//! Module for importing a archive into the current one

use regex::Regex;
use std::io::BufRead;

use crate::data::{
	json_archive::JSONArchive,
	provider::Provider,
	video::Video,
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
	merge_to: &mut JSONArchive,
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

/// Import a YTDL-Rust (json) Archive
///
/// This function modifies the input `archive`, and so will return `()`
pub fn import_ytdlr_json_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut JSONArchive,
	mut pgcb: S,
) -> Result<(), crate::Error> {
	log::debug!("import ytdl archive");

	pgcb(ImportProgress::Starting);

	let new_archive: JSONArchive = serde_json::from_reader(reader)?;

	pgcb(ImportProgress::SizeHint(new_archive.get_videos().len()));

	let mut successfull = 0usize;

	for (index, video) in new_archive.get_videos().iter().enumerate() {
		pgcb(ImportProgress::Increase(1, index));
		if merge_to.add_video(video.clone()) {
			successfull += 1;
		}
	}

	pgcb(ImportProgress::Finished(successfull));

	return Ok(());
}

lazy_static! {
	/// Regex for a line in a ytdl archive
	/// Ignores starting and ending whitespaces / tabs
	/// 1. capture group is the provider
	/// 2. capture group is the ID
	///
	/// Because the format of a ytdl-archive is not defined, it is rather loosely defines (any word character instead of specific characters)
	static ref YTDL_ARCHIVE_LINE_REGEX: Regex = Regex::new(r"(?mi)^\s*([\w\-_]+)\s+([\w\-_]+)\s*$").unwrap();
}

/// Import a YTDL Archive
///
/// This function modifies the input `archive`, and so will return `()`
pub fn import_ytdl_archive<T: BufRead, S: FnMut(ImportProgress)>(
	reader: &mut T,
	merge_to: &mut JSONArchive,
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
			if merge_to.add_video(
				Video::new(&cap[2], Provider::from(&cap[1]))
					.with_dl_finished(true) // add parsed video as having finished downloading, because it was already in the ytdl-archive
					.with_edit_asked(true), // add parsed video as having already been asked to edit, because no filename is available to ask for other edits
			) {
				successfull += 1;
				pgcb(ImportProgress::Increase(1, index));
			}
		} else {
			failed_captures = true;
			log::info!("Could not get any captures from line: \"{}\"", &line);

			continue;
		}
	}

	// Error if no valid lines have been found from the reader
	if successfull == 0 {
		return Err(crate::Error::NoCapturesFound(format!(
			"No valid lines have been found from the reader! Failed Captures: {}",
			failed_captures
		)));
	}

	pgcb(ImportProgress::Finished(successfull));

	return Ok(());
}

#[cfg(test)]
mod test {
	use super::*;
	use std::ops::Deref;
	use std::sync::RwLock;

	/// Test utility function for easy callbacks
	fn callback_counter(c: &RwLock<Vec<ImportProgress>>) -> impl FnMut(ImportProgress) + '_ {
		return |imp| c.write().expect("write failed").push(imp);
	}

	mod detect {
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

	mod any {
		use super::*;

		#[test]
		fn test_unexpected_eof() {
			let string0 = "";
			let mut dummy_archive = JSONArchive::default();

			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let ret = import_any_archive(
				&mut string0.as_bytes(),
				&mut dummy_archive,
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
			let mut archive0 = JSONArchive::default();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let res0 = import_any_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_ok());
			let cmp_archive0 = {
				let mut archive = JSONArchive::default();
				assert!(archive.add_video(
					Video::new("____________", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("------------", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("aaaaaaaaaaaa", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
						.with_dl_finished(true)
						.with_edit_asked(true),
				));

				archive
			};
			assert_eq!(cmp_archive0, archive0);
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
			let mut archive0 = JSONArchive::default();
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

			let res0 = import_any_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			assert_eq!(true, archive0.check_all_videos());

			let cmp_archive0 = {
				let mut archive = JSONArchive::default();
				assert!(archive.add_video(
					Video::new("____________", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true)
						.with_filename("someFile1.mp3"),
				));
				assert!(archive.add_video(
					Video::new("------------", Provider::Youtube)
						.with_dl_finished(false)
						.with_edit_asked(false)
						.with_filename("someFile2.mp3"),
				));
				assert!(archive.add_video(
					Video::new("aaaaaaaaaaaa", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(false)
						.with_filename("someFile3.mp3"),
				));
				assert!(archive.add_video(
					Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
						.with_dl_finished(true)
						.with_edit_asked(true)
						.with_filename("someFile4.mp3"),
				));

				archive
			};
			assert_eq!(cmp_archive0, archive0);
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

	mod ytdl {
		use super::*;

		#[test]
		fn test_basic_ytdl() {
			let mut archive0 = JSONArchive::default();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "
			youtube ____________
			youtube ------------
			youtube aaaaaaaaaaaa
			soundcloud 0000000000
			";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_ok());
			let cmp_archive0 = {
				let mut archive = JSONArchive::default();
				assert!(archive.add_video(
					Video::new("____________", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("------------", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("aaaaaaaaaaaa", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true),
				));
				assert!(archive.add_video(
					Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
						.with_dl_finished(true)
						.with_edit_asked(true),
				));

				archive
			};
			assert_eq!(cmp_archive0, archive0);
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
			let mut archive0 = JSONArchive::default();
			let pgcounter = RwLock::new(Vec::<ImportProgress>::new());

			let string0 = "";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_err());
			assert_eq!(
				Err(crate::Error::NoCapturesFound(
					"No valid lines have been found from the reader! Failed Captures: false".to_owned()
				)),
				res0
			);

			let string0 = "   ";

			let res0 = import_ytdl_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_err());
			assert_eq!(
				Err(crate::Error::NoCapturesFound(
					"No valid lines have been found from the reader! Failed Captures: true".to_owned()
				)),
				res0
			);
		}
	}

	mod ytdlr {
		use super::*;

		#[test]
		fn test_basic_ytdlr() {
			let mut archive0 = JSONArchive::default();
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

			let res0 = import_ytdlr_json_archive(&mut string0.as_bytes(), &mut archive0, callback_counter(&pgcounter));

			assert!(res0.is_ok());

			assert_eq!(true, archive0.check_all_videos());

			let cmp_archive0 = {
				let mut archive = JSONArchive::default();
				assert!(archive.add_video(
					Video::new("____________", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(true)
						.with_filename("someFile1.mp3"),
				));
				assert!(archive.add_video(
					Video::new("------------", Provider::Youtube)
						.with_dl_finished(false)
						.with_edit_asked(false)
						.with_filename("someFile2.mp3"),
				));
				assert!(archive.add_video(
					Video::new("aaaaaaaaaaaa", Provider::Youtube)
						.with_dl_finished(true)
						.with_edit_asked(false)
						.with_filename("someFile3.mp3"),
				));
				assert!(archive.add_video(
					Video::new("0000000000", Provider::Other("soundcloud".to_owned()))
						.with_dl_finished(true)
						.with_edit_asked(true)
						.with_filename("someFile4.mp3"),
				));

				archive
			};
			assert_eq!(cmp_archive0, archive0);
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
