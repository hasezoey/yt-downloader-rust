//! Module for importing a archive into the current one

use diesel::{
	prelude::*,
	upsert::excluded,
};
use regex::Regex;
use std::{
	fs::File,
	io::{
		BufRead,
		BufReader,
	},
	path::Path,
	sync::LazyLock,
};

use crate::{
	data::{
		UNKNOWN_NONE_PROVIDED,
		cache::media_provider::MediaProvider,
		sql_models::{
			InsMedia,
			Media,
		},
		sql_schema::media_archive,
	},
	error::IOErrorToError,
};

/// Enum to represent why the callback was called plus extra arguments
#[derive(Debug, PartialEq)]
pub enum ImportProgress {
	/// Variant that indicates that a process has started (clear / reset progress bar)
	/// Will always be called
	Starting,
	/// Variant for a Size-Hint [size hint]
	/// May not always be called
	SizeHint(usize),
	/// Variant for increasing the progress [increase by elements, current index]
	/// May not always be called
	///
	/// Note: the index may represent lines and not just elements
	Increase(usize, usize),
	/// Variant that indicates that a process has finished (finish progress bar) [successfull elements]
	/// Will always be called
	Finished(usize),
}

/// Archive Type, as detected by [`detect_archive_type`]
#[derive(Debug, PartialEq, Clone)]
pub enum ArchiveType {
	/// Unknown Archive type, may be a ytdl archive
	Unknown,
	/// JSON YTDL-R Archive
	JSON,
	/// SQLite YTDL-R Archive (currently unused)
	SQLite,
}

/// Detect what archive type the input reader's file is
pub fn detect_archive_type<T: BufRead>(reader: &mut T) -> Result<ArchiveType, crate::Error> {
	let buffer = reader.fill_buf().attach_location_err("detect_archive_type fill_buf")?; // read a bit of the reader, but dont consume the reader's contents

	if buffer.is_empty() {
		return Err(crate::Error::unexpected_eof(
			"Detected Empty File, Cannot detect format",
		));
	}

	// convert buffer to string, lossy, for trimming
	let as_string = String::from_utf8_lossy(buffer);

	let trimmed = as_string.trim_start().as_bytes();

	if trimmed.starts_with(b"{") {
		return Ok(ArchiveType::JSON);
	}

	if trimmed.starts_with(b"SQLite format") {
		return Ok(ArchiveType::SQLite);
	}

	return Ok(ArchiveType::Unknown);
}

/// Detect what archive is given and call the right function
///
/// This function modifies the input `merge_to` archive, and so will return `()`
pub fn import_any_archive<S: FnMut(ImportProgress)>(
	input_path: &Path,
	merge_to: &mut SqliteConnection,
	pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import any archive");

	let mut reader = BufReader::new(File::open(input_path).attach_path_err(input_path)?);

	return match detect_archive_type(&mut reader)? {
		ArchiveType::JSON => Err(crate::Error::other(
			"JSON archive is now unsupported, please use version <=0.10.0 to import it.",
		)),
		ArchiveType::SQLite => import_ytdlr_sqlite_archive(input_path, merge_to, pgcb),
		// Assume "Unknown" is a YTDL Archive (plain text)
		ArchiveType::Unknown => import_ytdl_archive(&mut reader, merge_to, pgcb),
	};
}

/// Import a YTDL-Rust (sqlite) Archive
///
/// This function modifies the input `merge_to` archive, and so will return `()`
pub fn import_ytdlr_sqlite_archive<S: FnMut(ImportProgress)>(
	input_path: &Path,
	merge_to: &mut SqliteConnection,
	mut pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import ytdl sqlite archive");

	// also applies migrations to input data before copying, because diesel can seemingly only support one version, and i dont want to implement handling for this
	let mut input_connection = crate::main::sql_utils::sqlite_connect(input_path)?;

	let max_id_result = media_archive::dsl::media_archive
		.select(diesel::dsl::max(media_archive::dsl::_id))
		.first::<Option<i64>>(&mut input_connection)?;

	pgcb(ImportProgress::Starting);

	if let Some(num) = max_id_result {
		// sqlite only support signed integers, but the rowid will always be positive (at least should be) and conversion should be possible
		let num = usize::try_from(num).map_err(|_| {
			return crate::Error::other(
				"Failed to convert column _id i64 to usize, expected the number to be positive",
			);
		})?;
		pgcb(ImportProgress::SizeHint(num));
	} else {
		log::warn!("Could not get the max_id of the input (got None)");
	}

	let mut affected_rows = 0usize;

	let lines_iter = media_archive::dsl::media_archive
		// order by oldest to newest
		.order(media_archive::inserted_at.asc())
		.load_iter::<Media, diesel::connection::DefaultLoadingMode>(&mut input_connection)?;

	for (index, val) in lines_iter.enumerate() {
		let val = val?;
		pgcb(ImportProgress::Increase(1, index));
		let insmedia = (&val).into();
		let affected = insert_insmedia(&insmedia, merge_to)?;

		affected_rows += affected;
	}

	pgcb(ImportProgress::Finished(affected_rows));

	return Ok(());
}

/// Regex for a line in a youtube-dl archive
/// Ignores starting and ending whitespaces / tabs
/// 1. capture group is the provider
/// 2. capture group is the ID
///
/// Because the format of a ytdl-archive is not defined, the regex is rather loosely defined (any word character instead of specific characters)
static YTDL_ARCHIVE_LINE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	return Regex::new(r"(?mi)^\s*([\w\-_]+)\s+([\w\-_]+)\s*$").unwrap();
});

/// Import a youtube-dl Archive
///
/// This function modifies the input `merge_to` archive, and so will return `()`
pub fn import_ytdl_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut SqliteConnection,
	mut pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import youtube-dl archive");

	pgcb(ImportProgress::Starting);

	let lines_iter = reader.lines();

	if let Some(size_hint) = lines_iter.size_hint().1 {
		pgcb(ImportProgress::SizeHint(size_hint));
	}

	let mut affected_rows = 0usize;
	let mut failed_captures = false;

	for (index, line) in lines_iter.enumerate() {
		// evaluate result, then redefine variable as without result
		let line = line.attach_location_err("import line iter")?;
		let line = line.trim();

		if line.is_empty() {
			continue;
		}

		if let Some(cap) = YTDL_ARCHIVE_LINE_REGEX.captures(line) {
			let affected = insert_insmedia(
				&InsMedia::new(&cap[2], MediaProvider::from(&cap[1]).as_str(), UNKNOWN_NONE_PROVIDED),
				merge_to,
			)?;

			affected_rows += affected;
			pgcb(ImportProgress::Increase(1, index));
		} else {
			failed_captures = true;
			log::info!("Could not get any captures from line: \"{}\"", &line);

			continue;
		}
	}

	// Error if no valid lines have been found from the reader
	if failed_captures {
		return Err(crate::Error::no_captures(
			"No valid lines have been found from the reader!",
		));
	}

	pgcb(ImportProgress::Finished(affected_rows));

	return Ok(());
}

/// Helper function to have a unified insertion command for all imports or functions that like to use this method
///
/// This function is also meant as a workaround to <https://github.com/diesel-rs/diesel/discussions/3115#discussioncomment-2509301> because bulk inserts with "on_conflict" in sqlite are not supported
#[inline]
pub fn insert_insmedia(input: &InsMedia, connection: &mut SqliteConnection) -> Result<usize, crate::Error> {
	return diesel::insert_into(media_archive::table)
		.values(input)
		.on_conflict((media_archive::media_id, media_archive::provider))
		.do_update()
		.set(media_archive::title.eq(excluded(media_archive::title)))
		.execute(connection)
		.map_err(|err| return crate::Error::from(err));
}

/// Helper function to have a unified insertion command for all imports or functions that like to use this method
/// This function does NOT update on conflict and ignores such values
#[inline]
pub fn insert_insmedia_noupdate(input: &InsMedia, connection: &mut SqliteConnection) -> Result<usize, crate::Error> {
	return diesel::insert_into(media_archive::table)
		.values(input)
		.on_conflict((media_archive::media_id, media_archive::provider))
		.do_nothing()
		.execute(connection)
		.map_err(|err| return crate::Error::from(err));
}

#[cfg(test)]
mod test {
	use super::*;
	use std::path::PathBuf;
	use std::sync::RwLock;
	use std::{
		io::Write,
		ops::Deref,
	};
	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	/// Test utility function for easy callbacks
	fn callback_counter(c: &RwLock<Vec<ImportProgress>>) -> impl FnMut(ImportProgress) + '_ {
		return |imp| c.write().expect("write failed").push(imp);
	}

	/// Test helper function to create a connection AND get a clean testing dir path
	fn create_connection() -> (SqliteConnection, TempDir) {
		let testdir = TempBuilder::new()
			.prefix("ytdl-test-import-")
			.tempdir()
			.expect("Expected a temp dir to be created");
		// chrono is used to create a different database for each thread
		let path = testdir.as_ref().join(format!("{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		return (
			crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start"),
			testdir,
		);
	}

	/// Test helper to create a input_data file with the provided data in the provided directory
	fn create_input_data(data: &str, input_dir: &Path) -> PathBuf {
		let file_path = input_dir.join("input_data");
		let mut file = std::fs::File::create(&file_path).expect("Expected file creation to not fail");

		file.write_all(data.as_bytes())
			.expect("Expected write_all to be successful");

		return file_path;
	}

	mod detect_archive_type {
		use super::*;

		#[test]
		fn test_eof() {
			let string0 = "";

			let ret0 = detect_archive_type(&mut string0.as_bytes());

			assert!(ret0.is_err());
			assert_eq!(
				Err(crate::Error::unexpected_eof(
					"Detected Empty File, Cannot detect format"
				)),
				ret0
			);
		}

		#[test]
		fn test_detect_json() {
			let string0 = "{}";

			let ret0 = detect_archive_type(&mut string0.as_bytes());

			assert!(ret0.is_ok());
			assert_eq!(Ok(ArchiveType::JSON), ret0);
		}

		#[test]
		fn test_detect_ytdl() {
			let string0 = "youtube ____________";

			let ret0 = detect_archive_type(&mut string0.as_bytes());

			assert!(ret0.is_ok());
			assert_eq!(Ok(ArchiveType::Unknown), ret0);

			let string1 = "soundcloud ____________";

			let ret1 = detect_archive_type(&mut string1.as_bytes());

			assert!(ret1.is_ok());
			assert_eq!(Ok(ArchiveType::Unknown), ret1);
		}

		#[test]
		fn test_detect_sqlite() {
			let string0 = "SQLite format 3";

			let ret0 = detect_archive_type(&mut string0.as_bytes());

			assert!(ret0.is_ok());
			assert_eq!(Ok(ArchiveType::SQLite), ret0);

			let string1 = "SQLite Format 3"; // this is case-sensitive

			let ret1 = detect_archive_type(&mut string1.as_bytes());

			assert!(ret1.is_ok());
			assert_eq!(Ok(ArchiveType::Unknown), ret1);
		}
	}

	mod import_any_archive {
		use super::*;

		#[test]
		fn test_unexpected_eof() {
			let string0 = "";
			let (mut dummy_connection, tempdir) = create_connection();
			let input_data_path = create_input_data(string0, tempdir.as_ref());

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let ret = import_any_archive(&input_data_path, &mut dummy_connection, callback_counter(&pgcounter));
			assert!(ret.is_err());
			assert_eq!(0, pgcounter.read().expect("read failed").len());
			assert_eq!(
				crate::Error::unexpected_eof("Detected Empty File, Cannot detect format"),
				ret.unwrap_err()
			);
		}

		#[test]
		fn test_any_to_ytdlr_ytdl() {
			let (mut connection0, tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";
			let input_data_path = create_input_data(string0, tempdir.as_ref());

			let res0 = import_any_archive(&input_data_path, &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());
			let cmp_vec: Vec<_> = vec![
				("____________", "youtube", UNKNOWN_NONE_PROVIDED),
				("------------", "youtube", UNKNOWN_NONE_PROVIDED),
				("aaaaaaaaaaaa", "youtube", UNKNOWN_NONE_PROVIDED),
				("0000000000", "soundcloud", UNKNOWN_NONE_PROVIDED),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
			assert_eq!(
				&vec![
					ImportProgress::Starting,
					// index does not start at "0", because of the empty first line in "string0"
					ImportProgress::Increase(1, 1),
					ImportProgress::Increase(1, 2),
					ImportProgress::Increase(1, 3),
					ImportProgress::Increase(1, 4),
					ImportProgress::Finished(4)
				],
				pgcounter.read().expect("failed to read").deref()
			);
		}

		#[test]
		fn test_any_to_ytdlr_sqlite() {
			let (mut connection0, tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let media = &[
				InsMedia::new("____________", "youtube", "Some Title1"),
				InsMedia::new("------------", "youtube", "Some Title2"),
				InsMedia::new("aaaaaaaaaaaa", "youtube", "Some Title3"),
				InsMedia::new("0000000000", "soundcloud", "Some Title4"),
			];

			let import_from_path = tempdir.path().join("from.db");

			let mut import_from_connection = crate::main::sql_utils::sqlite_connect(&import_from_path)
				.expect("Expected SQLite to successfully start");

			for insmedia in media {
				insert_insmedia(insmedia, &mut import_from_connection).unwrap();
			}

			drop(import_from_connection);

			let res0 = import_any_archive(&import_from_path, &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let cmp_vec: Vec<_> = vec![
				("____________", "youtube", "Some Title1"),
				("------------", "youtube", "Some Title2"),
				("aaaaaaaaaaaa", "youtube", "Some Title3"),
				("0000000000", "soundcloud", "Some Title4"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
			assert_eq!(
				&vec![
					ImportProgress::Starting,
					ImportProgress::SizeHint(4), // Size Hint of 4, because of a intermediate array length
					// index start at 0, thanks to json array index
					ImportProgress::Increase(1, 0),
					ImportProgress::Increase(1, 1),
					ImportProgress::Increase(1, 2),
					ImportProgress::Increase(1, 3),
					ImportProgress::Finished(4)
				],
				pgcounter.read().expect("failed to read").deref()
			);
		}
	}

	mod insert_insmedia {
		use super::*;

		#[test]
		fn test_insert() {
			let (mut connection0, _tempdir) = create_connection();

			let input0 = InsMedia::new("someid", "someprovider", "sometitle");

			let res = insert_insmedia(&input0, &mut connection0);

			assert!(res.is_ok());
			let res = res.expect("Expected assert to fail before this");

			assert_eq!(1, res);

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			let cmp_vec: Vec<_> = vec![("someid", "someprovider", "sometitle")];

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
		}
	}

	mod import_ytdl_archive {
		use super::*;

		#[test]
		fn test_basic_ytdl() {
			let (mut connection0, _tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());
			let cmp_vec: Vec<_> = vec![
				("____________", "youtube", UNKNOWN_NONE_PROVIDED),
				("------------", "youtube", UNKNOWN_NONE_PROVIDED),
				("aaaaaaaaaaaa", "youtube", UNKNOWN_NONE_PROVIDED),
				("0000000000", "soundcloud", UNKNOWN_NONE_PROVIDED),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
			assert_eq!(
				&vec![
					ImportProgress::Starting,
					// index does not start at "0", because of the empty first line in "string0"
					ImportProgress::Increase(1, 1),
					ImportProgress::Increase(1, 2),
					ImportProgress::Increase(1, 3),
					ImportProgress::Increase(1, 4),
					ImportProgress::Finished(4)
				],
				pgcounter.read().expect("failed to read").deref()
			);
		}

		#[test]
		fn test_no_captures_found_err() {
			let (mut connection0, _tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let string0 = "notvalid";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_err());
			assert_eq!(
				Err(crate::Error::no_captures(
					"No valid lines have been found from the reader!"
				)),
				res0
			);
		}
	}

	mod import_ytdlr_sqlite_archive {
		use super::*;

		/// Test helper function to create a connection AND get a clean testing dir path
		fn create_connection_input(data: &[InsMedia], temp_dir: &Path) -> PathBuf {
			// chrono is used to create a different database for each thread
			let path = temp_dir.join(format!("{}-input-sqlite.db", chrono::Utc::now()));

			// remove if already exists to have a clean test
			if path.exists() {
				std::fs::remove_file(&path).expect("Expected the file to be removed");
			}

			let mut connection =
				crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start");

			for data in data {
				insert_insmedia(data, &mut connection).expect("Expected Successful insert");
			}

			return path;
		}

		#[test]
		fn test_basic_ytdlr_sqlite() {
			let (mut connection0, tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let insert_data = &[
				InsMedia::new("____________", "youtube", "someTitle1"),
				InsMedia::new("------------", "youtube", "someTitle2"),
				InsMedia::new("aaaaaaaaaaaa", "youtube", "someTitle3"),
				InsMedia::new("0000000000", "soundcloud", "someTitle4"),
			];

			let input_sqlite_path = create_connection_input(insert_data, tempdir.as_ref());

			let res0 = import_ytdlr_sqlite_archive(&input_sqlite_path, &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let cmp_vec: Vec<_> = vec![
				("____________", "youtube", "someTitle1"),
				("------------", "youtube", "someTitle2"),
				("aaaaaaaaaaaa", "youtube", "someTitle3"),
				("0000000000", "soundcloud", "someTitle4"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
			assert_eq!(
				&vec![
					ImportProgress::Starting,
					ImportProgress::SizeHint(4), // Size Hint of 4, because of a intermediate array length
					// index start at 0, thanks to json array index
					ImportProgress::Increase(1, 0),
					ImportProgress::Increase(1, 1),
					ImportProgress::Increase(1, 2),
					ImportProgress::Increase(1, 3),
					ImportProgress::Finished(4)
				],
				pgcounter.read().expect("failed to read").deref()
			);
		}

		#[test]
		fn test_conflict() {
			let (mut connection0, tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			// insert conflicting data
			insert_insmedia(
				&InsMedia::new("____________", "youtube", "someTitle1"),
				&mut connection0,
			)
			.expect("Expected insert to be successful");

			let insert_data = &[
				InsMedia::new("____________", "youtube", "some A title"), // different title
				InsMedia::new("------------", "youtube", "someTitle2"),
				InsMedia::new("aaaaaaaaaaaa", "youtube", "someTitle3"),
				InsMedia::new("0000000000", "soundcloud", "someTitle4"),
			];

			let input_sqlite_path = create_connection_input(insert_data, tempdir.as_ref());

			let res0 = import_ytdlr_sqlite_archive(&input_sqlite_path, &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let cmp_vec: Vec<_> = vec![
				// should have the title updated
				("____________", "youtube", "some A title"),
				("------------", "youtube", "someTitle2"),
				("aaaaaaaaaaaa", "youtube", "someTitle3"),
				("0000000000", "soundcloud", "someTitle4"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(
				cmp_vec,
				found
					.iter()
					.map(|v| return (v.media_id.as_str(), v.provider.as_str(), v.title.as_str()))
					.collect::<Vec<_>>()
			);
		}
	}
}
