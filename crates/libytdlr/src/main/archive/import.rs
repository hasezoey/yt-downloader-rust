//! Module for importing a archive into the current one

use diesel::{
	prelude::*,
	upsert::excluded,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::io::BufRead;

use crate::data::{
	old_archive::{
		JSONArchive,
		Provider,
	},
	sql_models::*,
	sql_schema::*,
};

/// Enum to represent why the callback was called plus extra arguments
#[derive(Debug, PartialEq)]
#[non_exhaustive]
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
	let buffer = reader.fill_buf()?; // read a bit of the reader, but dont consume the reader's contents

	if buffer.is_empty() {
		return Err(crate::Error::UnexpectedEOF(
			"Detected Empty File, Cannot detect format".to_owned(),
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
/// Calls [`import_ytdl_archive`] when its a ytdl archive and [`import_ytdlr_json_archive`] when its a ytdl-r archive
///
/// This function modifies the input `archive`, and so will return `()`
pub fn import_any_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut SqliteConnection,
	pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import any archive");

	return match detect_archive_type(reader)? {
		ArchiveType::JSON => import_ytdlr_json_archive(reader, merge_to, pgcb),
		ArchiveType::SQLite => todo!(),
		// Assume "Unknown" is a YTDL Archive (plain text)
		ArchiveType::Unknown => import_ytdl_archive(reader, merge_to, pgcb),
	};
}

/// Regex for removing known file extension from imported filenames
/// for [import_ytdlr_json_archive]
static REMOVE_KNOWN_FILEEXTENSION: Lazy<Regex> = Lazy::new(|| {
	return Regex::new(r"(?mi)\.(?:(?:mp3)|(?:mp4))$").unwrap();
});

/// Import a YTDL-Rust (json) Archive
///
/// This function modifies the input `archive`, and so will return `()`
pub fn import_ytdlr_json_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut SqliteConnection,
	mut pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import ytdl archive");

	pgcb(ImportProgress::Starting);

	let input_archive: JSONArchive = serde_json::from_reader(reader)?;

	pgcb(ImportProgress::SizeHint(input_archive.get_videos().len()));

	let mut bulk_values: Vec<InsMedia> = Vec::with_capacity(input_archive.get_videos().len());

	for (index, video) in input_archive.get_videos().iter().enumerate() {
		pgcb(ImportProgress::Increase(1, index));

		let filename = REMOVE_KNOWN_FILEEXTENSION.replace_all(video.file_name(), "");

		bulk_values.push(InsMedia::new(video.id(), video.provider().to_string(), filename));
	}

	// currently does not work, see https://github.com/diesel-rs/diesel/discussions/3115
	// let affected_rows = diesel::insert_into(media_archive::table)
	// 	.values(&bulk_values)
	// 	.on_conflict((media_id, provider))
	// 	.do_update()
	// 	.set(title.eq(excluded(title)))
	// 	.execute(merge_to)
	// 	.map_err(|err| return crate::Error::SQLOperationError(err.to_string()))?;

	let mut affected_rows = 0usize;

	// HACK: the following is currently just a workaround because of https://github.com/diesel-rs/diesel/discussions/3115#discussioncomment-2509301
	for val in bulk_values.iter() {
		let affected = insert_insmedia(val, merge_to)?;

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
static YTDL_ARCHIVE_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
	return Regex::new(r"(?mi)^\s*([\w\-_]+)\s+([\w\-_]+)\s*$").unwrap();
});

/// Import a youtube-dl Archive
///
/// This function modifies the input `archive`, and so will return `()`
pub fn import_ytdl_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut SqliteConnection,
	mut pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import ytdl-rust archive");

	pgcb(ImportProgress::Starting);

	let lines_iter = reader.lines();

	if let Some(size_hint) = lines_iter.size_hint().1 {
		pgcb(ImportProgress::SizeHint(size_hint));
	}

	let mut successfull = 0usize;
	let mut failed_captures = false;

	for (index, line) in lines_iter.enumerate() {
		// evaluate result, then redefine variable as without result
		let line = line?;

		if let Some(cap) = YTDL_ARCHIVE_LINE_REGEX.captures(&line) {
			let affected = insert_insmedia(
				&InsMedia::new(&cap[2], Provider::from(&cap[1]).to_string(), "unknown (none-provided)"),
				merge_to,
			)?;

			successfull += affected;
			pgcb(ImportProgress::Increase(1, index));
		} else {
			failed_captures = true;
			log::info!("Could not get any captures from line: \"{}\"", &line);

			continue;
		}
	}

	// Error if no valid lines have been found from the reader
	if successfull == 0 {
		return Err(crate::Error::NoCapturesFound(format!(
			"No valid lines have been found from the reader! Failed Captures: {failed_captures}"
		)));
	}

	pgcb(ImportProgress::Finished(successfull));

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
		.map_err(|err| return crate::Error::SQLOperationError(err.to_string()));
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::data::old_archive::video::Video;
	use std::ops::Deref;
	use std::sync::RwLock;
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

	mod detect_archive_type {
		use super::*;

		#[test]
		fn test_eof() {
			let string0 = "";

			let ret0 = detect_archive_type(&mut string0.as_bytes());

			assert!(ret0.is_err());
			assert_eq!(
				Err(crate::Error::UnexpectedEOF(
					"Detected Empty File, Cannot detect format".to_owned()
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
			let (mut dummy_connection, _tempdir) = create_connection();

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let ret = import_any_archive(
				&mut string0.as_bytes(),
				&mut dummy_connection,
				callback_counter(&pgcounter),
			);
			assert!(ret.is_err());
			assert_eq!(0, pgcounter.read().expect("read failed").len());
			assert_eq!(
				crate::Error::UnexpectedEOF("Detected Empty File, Cannot detect format".to_owned()),
				ret.unwrap_err()
			)
		}

		#[test]
		fn test_any_to_ytdl() {
			let (mut connection0, _tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let res0 = import_any_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());
			let cmp_vec: Vec<Video> = vec![
				Video::new("____________", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("------------", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("aaaaaaaaaaaa", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(cmp_vec, found.iter().map(Video::from).collect::<Vec<Video>>());
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
		fn test_any_to_ytdlr() {
			let (mut connection0, _tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = r#"
			{
				"version": "0.1.0",
				"videos": [
					{
						"id": "____________",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile1.mp3"
					},
					{
						"id": "------------",
						"provider": "youtube",
						"dlFinished": false,
						"editAsked": true,
						"fileName": "someFile2.mp3"
					},
					{
						"id": "aaaaaaaaaaaa",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": false,
						"fileName": "someFile3.mp3"
					},
					{
						"id": "0000000000",
						"provider": "soundcloud",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile4.mp3"
					}
				]
			}
			"#;

			let res0 = import_any_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let cmp_vec: Vec<Video> = vec![
				Video::new("____________", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile1"),
				Video::new("------------", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile2"),
				Video::new("aaaaaaaaaaaa", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile3"),
				Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile4"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(cmp_vec, found.iter().map(Video::from).collect::<Vec<Video>>());
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

			let cmp_vec: Vec<Video> = vec![Video::new("someid", Provider::Other("someprovider".to_owned()))
				.with_dl_finished(true)
				.with_edit_asked(true)
				.with_filename("sometitle")];

			assert_eq!(cmp_vec, found.iter().map(Video::from).collect::<Vec<Video>>());
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
			let cmp_vec: Vec<Video> = vec![
				Video::new("____________", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("------------", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("aaaaaaaaaaaa", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
				Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("unknown (none-provided)"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(cmp_vec, found.iter().map(Video::from).collect::<Vec<Video>>());
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

			assert!(res0.is_err());
			assert_eq!(
				Err(crate::Error::NoCapturesFound(
					"No valid lines have been found from the reader! Failed Captures: false".to_owned()
				)),
				res0
			);

			let string0 = "   ";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_err());
			assert_eq!(
				Err(crate::Error::NoCapturesFound(
					"No valid lines have been found from the reader! Failed Captures: true".to_owned()
				)),
				res0
			);
		}
	}

	mod import_ytdlr_json_archive {
		use super::*;

		#[test]
		fn test_basic_ytdlr() {
			let (mut connection0, _tempdir) = create_connection();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = r#"
			{
				"version": "0.1.0",
				"videos": [
					{
						"id": "____________",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile1.mp3"
					},
					{
						"id": "------------",
						"provider": "youtube",
						"dlFinished": false,
						"editAsked": true,
						"fileName": "someFile2.mp3"
					},
					{
						"id": "aaaaaaaaaaaa",
						"provider": "youtube",
						"dlFinished": true,
						"editAsked": false,
						"fileName": "someFile3.mp3"
					},
					{
						"id": "0000000000",
						"provider": "soundcloud",
						"dlFinished": true,
						"editAsked": true,
						"fileName": "someFile4.mp3"
					}
				]
			}
			"#;

			let res0 =
				import_ytdlr_json_archive(&mut string0.as_bytes(), &mut connection0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			let cmp_vec: Vec<Video> = vec![
				Video::new("____________", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile1"),
				Video::new("------------", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile2"),
				Video::new("aaaaaaaaaaaa", Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile3"),
				Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("someFile4"),
			];

			let found = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				.load::<Media>(&mut connection0)
				.expect("Expected a successfully query");

			assert_eq!(cmp_vec, found.iter().map(Video::from).collect::<Vec<Video>>());
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
}
