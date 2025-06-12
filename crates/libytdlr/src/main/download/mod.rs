//! Module for handling youtube-dl

use assemble_cmd::assemble_ytdl_command;
use chrono::NaiveDate;
use diesel::SqliteConnection;
use parse_linetype::{
	CustomParseType,
	LineType,
};
use std::{
	fs::OpenOptions,
	io::{
		BufRead,
		BufReader,
		BufWriter,
		Write,
	},
	time::Duration,
};

use crate::{
	data::cache::media_info::MediaInfo,
	error::IOErrorToError,
	spawn::ytdl::YTDL_BIN_NAME,
};

pub use download_options::{
	DownloadOptions,
	FormatArgument,
};

mod assemble_cmd;
mod download_options;
mod parse_linetype;

/// The minimal youtube-dl(p) version that is expected to be used.
///
/// Newer versions can be used to likely unlock extra functionality, but ytdlr is build around this as the minimal in mind.
pub const MINIMAL_YTDL_VERSION: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2023, 3, 3).unwrap();

/// Types for [DownloadProgress::Skipped]
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum SkippedType {
	/// Skipped because of a Error
	Error,
	/// Skipped because already being in the archive
	InArchive,
}

/// Enum for hooks to know what is currently happening
/// All Variants will have a certian order in which they are called (like AllStarting is always before a SingleStarting)
/// but not all may be called, like there may be "SingleStarting -> SingleProgress -> Skipped" instead of "SingleStarting -> SingleProgress -> SingleFinished"
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadProgress {
	/// Variant representing that the download of a single url is starting
	UrlStarting,
	/// Variant representing a skipped element, may or may not come because of it already being in the archive
	/// may be called after "SingleStarting" and / or "SingleProcess" instead of "SingleFinished"
	/// values (skipped_count)
	Skipped(usize, SkippedType),
	/// Variant representing that a media has started the process
	/// values: (id, title)
	SingleStarting(String, String),
	/// Variant representing that a started media has increased in progress
	/// "id" may be [`None`] when the previous parsing did not parse a title
	/// values:  (id, progress)
	SingleProgress(Option<String>, u8),
	/// Variant representing that a media has finished the process
	/// the "id" is not guranteed to be the same as in [`DownloadProgress::SingleStarting`]
	/// will only be called if there was a download AND no error happened
	/// values: (id)
	SingleFinished(String),
	/// Variant representing that the download of a single url has finished
	/// The value in this tuple is the size of actually downloaded media, not just found media
	/// values: (downloaded media count)
	UrlFinished(usize),
	/// Variant representing that playlist info has been found - may not trigger if not in a playlist
	/// the first (and currently only) value is the count of media in the playlist
	/// values: (playlist_count)
	PlaylistInfo(usize),
}

/// Warn if a version lower than the minimal is used
fn warn_minimal_version(ytdl_version: NaiveDate) {
	if ytdl_version < MINIMAL_YTDL_VERSION {
		warn!(
			"Used {} version ({}) is lower than the recommended minimal {}",
			YTDL_BIN_NAME,
			ytdl_version.format("%Y.%m.%d"),
			MINIMAL_YTDL_VERSION.format("%Y.%m.%d"),
		);
	}
}

/// Download a single URL
/// Assumes ytdl and ffmpeg have already been checked to exist and work (like using [`crate::spawn::ytdl::ytdl_version`])
/// Adds all non-skipped Media to the input [`Vec<MediaInfo>`]
pub fn download_single<A: DownloadOptions, C: FnMut(DownloadProgress)>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
	pgcb: C,
	mediainfo_vec: &mut Vec<MediaInfo>,
) -> Result<(), crate::Error> {
	warn_minimal_version(options.ytdl_version());

	let ytdl_child = {
		let args = assemble_ytdl_command(connection, options)?;

		// merge stderr into stdout
		duct::cmd(YTDL_BIN_NAME, args)
			.stderr_to_stdout()
			.reader()
			.attach_location_err("duct ytdl reader")?
	};

	let stdout_reader = BufReader::new(&ytdl_child);

	handle_stdout(options, pgcb, stdout_reader, mediainfo_vec)?;

	loop {
		// wait loop, because somehow a "ReaderHandle" does not implement "wait", only "try_wait", but have to wait for it to exit here
		match ytdl_child.try_wait() {
			Ok(v) => {
				// only in the "Some" case is the wait actually finished
				if v.is_some() {
					break;
				}
			},
			Err(err) => {
				// ignore duct errors as non-"Err" worthy
				warn!("youtube-dl exited with a non-0 code: {err}");
				break;
			},
		}

		std::thread::sleep(Duration::from_millis(100)); // sleep to same some time between the next wait (to not cause constant cpu spike)
	}

	return Ok(());
}

/// Youtube-DL archive prefix
pub const YTDL_ARCHIVE_PREFIX: &str = "ytdl_archive_";
/// Youtube-DL archive extension
pub const YTDL_ARCHIVE_EXT: &str = ".txt";

/// Consistent way of getting the archive name
#[must_use]
pub fn get_archive_name(output_dir: &std::path::Path) -> std::path::PathBuf {
	return output_dir.join(format!(
		"{}{}{}",
		YTDL_ARCHIVE_PREFIX,
		std::process::id(),
		YTDL_ARCHIVE_EXT
	));
}

/// Helper function to handle the output from a spawned ytdl command
/// Adds all non-skipped Media to the input [`Vec<MediaInfo>`]
#[inline]
fn handle_stdout<A: DownloadOptions, C: FnMut(DownloadProgress), R: BufRead>(
	options: &A,
	mut pgcb: C,
	reader: R,
	mediainfo_vec: &mut Vec<MediaInfo>,
) -> Result<(), crate::Error> {
	// report that the downloading is now starting
	pgcb(DownloadProgress::UrlStarting);

	// cache the bool for "print_command_stdout" to not execute the function for every line (should be a static value)
	let print_stdout = options.print_command_log();

	// the array where finished "current_mediainfo" gets appended to
	// for performance / allocation efficiency, a count is requested from options
	// let mut mediainfo_vec: Vec<MediaInfo> = Vec::with_capacity(options.get_count_estimate());
	// "current_mediainfo" may not be defined because it cannot be guranteed that a parsed output was emitted
	let mut current_mediainfo: Option<MediaInfo> = None;
	// value to determine if a media has actually been downloaded, or just found
	let mut had_download = false;
	// store the last error line encountered
	let mut last_error = None;

	let mut maybe_command_file_log = if options.save_command_log() {
		let path = options
			.download_path()
			.join(format!("yt-dl_{}.log", std::process::id()));

		info!("Logging command output to \"{}\"", path.display());

		let mut file = BufWriter::new(
			OpenOptions::new()
				.create(true)
				.append(true)
				.open(&path)
				.attach_path_err(&path)?,
		);

		file.write_all(b"\nNew Instance\n").attach_path_err(&path)?;

		Some((file, path))
	} else {
		None
	};

	// HACK: .lines() iter never exits on non-0 exit codes in duct, see https://github.com/oconnor663/duct.rs/issues/112
	for line in reader.lines() {
		let line = match line {
			Ok(v) => v,
			Err(err) => {
				debug!("duct lines reader errored: {}", err);
				break; // handle it as a non-breaking case, because in 99% of cases it is just a error of "command ... exited with code ?"
			},
		};

		// only print STDOUT to output when requested
		if print_stdout {
			trace!("ytdl [STDOUT]: \"{}\"", line);
		}
		if let Some((file, path)) = &mut maybe_command_file_log {
			file.write_all(line.as_bytes()).attach_path_err(&path)?;
			file.write_all(b"\n").attach_path_err(path)?;
		}

		if let Some(linetype) = LineType::try_from_line(&line) {
			// clear last_error line once the linetype is not error anymore (like in playlist to not fail if the playlist is not just skipped / private media)
			if linetype != LineType::Error {
				last_error = None;
			}
			match linetype {
				// currently there is nothing that needs to be done with "Ffmpeg" lines
				LineType::Ffmpeg
				// currently there is nothing that needs to be done with "ProviderSpecific" Lines, thanks to "--print"
				| LineType::ProviderSpecific
				// currently there is nothing that needs to be done with "Generic" Lines
				| LineType::Generic => (),
				LineType::Download => {
					had_download = true;
					if let Some(percent) = linetype.try_get_download_percent(line) {
						// convert "current_mediainfo" to a reference and operate on the inner value (if exists) to return just the "id"
						let id = current_mediainfo.as_ref().map(|v| return v.id.clone());
						pgcb(DownloadProgress::SingleProgress(id, percent));
					}
				},
				LineType::Custom => handle_linetype_custom(linetype, &line, &mut current_mediainfo, &mut pgcb, &mut had_download, mediainfo_vec),
				LineType::ArchiveSkip => {
					pgcb(DownloadProgress::Skipped(1, SkippedType::InArchive));
				},
				LineType::Error => {
					// the following is using debug printing, because the line may include escape characters, which would mess-up the printing, but is still good to know when reading
					warn!("Encountered youtube-dl error: {:#?}", line);
					last_error = Some(crate::Error::other(line));
					pgcb(DownloadProgress::Skipped(1, SkippedType::Error));
					current_mediainfo.take(); // replace with none, because this media should not be added
				},
				LineType::Warning => {
					// ytdl warnings are non-fatal, but should still be logged
					warn!("youtube-dl: {:#?}", line);
				}
			}
		} else if !line.is_empty() {
			info!("No type has been found for line \"{}\"", line);
		}
	}

	// report that downloading is now finished
	pgcb(DownloadProgress::UrlFinished(mediainfo_vec.len()));

	if let Some(last_error) = last_error {
		return Err(last_error);
	}

	return Ok(());
}

/// Handle [LineType::Custom]
///
/// outsourced, because it would otherwise become really nested
fn handle_linetype_custom<C: FnMut(DownloadProgress)>(
	linetype: LineType,
	line: &str,
	current_mediainfo: &mut Option<MediaInfo>,
	mut pgcb: C,
	had_download: &mut bool,
	mediainfo_vec: &mut Vec<MediaInfo>,
) {
	if let Some(parsed_type) = linetype.try_get_parse_helper(line) {
		match parsed_type {
			CustomParseType::Start(mi) => {
				debug!(
					"Found PARSE_START: \"{}\" \"{}\" \"{:?}\"",
					mi.id, mi.provider, mi.title
				);
				if current_mediainfo.is_some() {
					warn!("Found PARSE_START, but \"current_mediainfo\" is still \"Some\"");
				}
				current_mediainfo.replace(mi);
				// the following uses "unwrap", because the option has been set by the previous line
				let c_mi = current_mediainfo.as_ref().unwrap();
				// the following also uses "expect", because "try_get_parse_helper" is guranteed to return with id, title, provider for "PARSE_START"
				let title = c_mi
					.title
					.as_ref()
					.expect("current_mediainfo.title should have been set");
				pgcb(DownloadProgress::SingleStarting(c_mi.id.clone(), title.to_string()));
			},
			CustomParseType::End(mi) => {
				debug!("Found PARSE_END: \"{}\" \"{}\"", mi.id, mi.provider);

				if let Some(last_mediainfo) = current_mediainfo.take() {
					pgcb(DownloadProgress::SingleFinished(mi.id.clone())); // callback inside here, because it should only be triggered if there was a media_info to take
					if mi.id != last_mediainfo.id {
						// warn in the weird case where the "current_mediainfo" and result from PARSE_END dont match
						warn!("Found PARSE_END, but the ID does dont match with \"current_mediainfo\"!");
					}

					// do not add videos to "mediainfo_vec", unless the media had actually been downloaded
					if *had_download {
						mediainfo_vec.push(last_mediainfo);
					}
				} else {
					// write a log that PARSE_END was present but was None (like in the case of a Error happening)
					debug!("Found a PARSE_END, but \"current_mediainfo\" was \"None\"!");
				}

				// reset the value for the next download
				*had_download = false;
			},
			CustomParseType::Playlist(count) => {
				debug!("Found PLAYLIST {count}");
				pgcb(DownloadProgress::PlaylistInfo(count));
			},
			CustomParseType::Move(mi) => {
				debug!("Found MOVE: \"{}\" \"{}\" \"{:?}\"", mi.id, mi.provider, mi.filename);

				if let Some(last_mediainfo) = current_mediainfo.as_mut() {
					last_mediainfo.set_filename(
						mi.filename
							.expect("Expected try_get_parse_helper to return a mediainfo with filename"),
					);
				} else {
					warn!("Found MOVE, but did not have a current_mediainfo");
				}
			},
		}
	}
}

#[cfg(test)]
pub(crate) mod test_utils {
	use std::{
		path::PathBuf,
		sync::{
			Arc,
			atomic::AtomicUsize,
		},
	};

	use diesel::SqliteConnection;
	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	use super::{
		DownloadProgress,
		download_options::{
			DownloadOptions,
			FormatArgument,
		},
	};

	/// Test Implementation for [`DownloadOptions`]
	pub struct TestOptions {
		pub audio_only:        bool,
		pub extra_arguments:   Vec<PathBuf>,
		pub download_path:     PathBuf,
		pub url:               String,
		pub archive_lines:     Vec<String>,
		pub print_command_log: bool,
		pub save_command_log:  bool,
		pub sub_langs:         Option<String>,
		pub ytdl_version:      chrono::NaiveDate,

		pub audio_format: FormatArgument<'static>,
		pub video_format: FormatArgument<'static>,
	}

	impl TestOptions {
		/// Helper Function for easily creating a new instance of [`TestOptions`] for [`assemble_ytdl_command`] testing
		pub fn new_assemble(
			audio_only: bool,
			extra_arguments: Vec<PathBuf>,
			download_path: PathBuf,
			url: String,
			archive_lines: Vec<String>,
		) -> Self {
			return Self {
				audio_only,
				extra_arguments,
				download_path,
				url,
				archive_lines,
				..Default::default()
			};
		}

		/// Helper Function for easily creating a new instance of [`TestOptions`] for [`handle_stdout`] testing
		pub fn new_handle_stdout(print_command_log: bool) -> Self {
			return Self {
				print_command_log,
				..Default::default()
			};
		}

		/// Test with a custom ytdl_version
		pub fn with_version(mut self, ytdl_version: chrono::NaiveDate) -> Self {
			self.ytdl_version = ytdl_version;

			return self;
		}

		/// Get the test default version
		pub fn default_version() -> chrono::NaiveDate {
			// return current date plus 1 year to activate all features for now
			return chrono::offset::Utc::now()
				.naive_utc()
				.checked_add_months(chrono::Months::new(12))
				.unwrap()
				.into();
		}

		/// Set custom audio & video formats
		pub fn set_format(self, audio_format: FormatArgument<'static>, video_format: FormatArgument<'static>) -> Self {
			return Self {
				audio_format,
				video_format,
				..self
			};
		}
	}

	impl Default for TestOptions {
		fn default() -> Self {
			return Self {
				audio_only:        false,
				extra_arguments:   Vec::default(),
				download_path:     PathBuf::default(),
				url:               String::default(),
				archive_lines:     Vec::default(),
				print_command_log: false,
				save_command_log:  false,
				sub_langs:         None,
				ytdl_version:      Self::default_version(),

				audio_format: "mp3",
				video_format: "mkv",
			};
		}
	}

	impl DownloadOptions for TestOptions {
		fn audio_only(&self) -> bool {
			return self.audio_only;
		}

		fn download_path(&self) -> &std::path::Path {
			return &self.download_path;
		}

		fn get_url(&self) -> &str {
			return &self.url;
		}

		fn gen_archive(&self, _connection: &mut SqliteConnection) -> Option<Box<dyn Iterator<Item = String> + '_>> {
			if self.archive_lines.is_empty() {
				return None;
			}

			return Some(Box::from(self.archive_lines.iter().map(|v| return v.clone())));
		}

		fn extra_ytdl_arguments(&self) -> Vec<&std::ffi::OsStr> {
			return self.extra_arguments.iter().map(|v| return v.as_os_str()).collect();
		}

		fn print_command_log(&self) -> bool {
			return self.print_command_log;
		}

		fn save_command_log(&self) -> bool {
			return self.save_command_log;
		}

		fn sub_langs(&self) -> Option<&str> {
			return self.sub_langs.as_deref();
		}

		fn ytdl_version(&self) -> chrono::NaiveDate {
			return self.ytdl_version;
		}

		fn get_audio_format(&self) -> FormatArgument {
			return self.audio_format;
		}

		fn get_video_format(&self) -> FormatArgument {
			return self.video_format;
		}
	}

	/// Test helper function to create a connection AND get a clean testing dir path
	pub fn create_connection() -> (SqliteConnection, TempDir, PathBuf) {
		let testdir = TempBuilder::new()
			.prefix("ytdl-test-download-")
			.tempdir()
			.expect("Expected a temp dir to be created");
		// chrono is used to create a different database for each thread
		let path = testdir.as_ref().join(format!("{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		let parent = testdir.as_ref().to_owned();

		return (
			crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start"),
			testdir,
			parent,
		);
	}

	/// Test utility function for easy callbacks
	pub fn callback_counter<'a>(
		index_pg: &'a Arc<AtomicUsize>,
		expected_pg: &'a [DownloadProgress],
	) -> impl FnMut(DownloadProgress) + 'a {
		return |imp| {
			let index = index_pg.load(std::sync::atomic::Ordering::Relaxed);
			// panic in case there are more events than expected, with a more useful message than default
			assert!(
				index <= expected_pg.len(),
				"index_pg is higher than provided expected_pg values! (more events than expected?)"
			);
			assert_eq!(expected_pg[index], imp);
			index_pg.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
		};
	}
}

#[cfg(test)]
mod test {
	use std::sync::Arc;
	use std::sync::atomic::AtomicUsize;

	use super::*;

	mod handle_stdout {
		use test_utils::{
			TestOptions,
			callback_counter,
		};

		use super::*;

		#[test]
		fn test_basic_single_usage() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::SingleStarting("-----------".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleFinished("-----------".to_owned()),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
PARSE_START 'youtube' '-----------' Some Title Here
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '-----------'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("-----------", "youtube").with_title("Some Title Here")],
				media_vec
			);
		}

		#[test]
		fn test_basic_multi_usage() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::SingleStarting("----------0".to_owned(), "Some Title Here 0".to_owned()),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleFinished("----------0".to_owned()),
				DownloadProgress::SingleStarting("----------1".to_owned(), "Some Title Here 1".to_owned()),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleFinished("----------1".to_owned()),
				DownloadProgress::UrlFinished(2),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
PARSE_START 'youtube' '----------0' Some Title Here 0
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '----------0'
PARSE_START 'soundcloud' '----------1' Some Title Here 1
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'soundcloud' '----------1'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(2, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("----------0", "youtube").with_title("Some Title Here 0"),
					MediaInfo::new("----------1", "soundcloud").with_title("Some Title Here 1")
				],
				media_vec
			);
		}

		#[test]
		fn test_skipped() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::SingleStarting("-----------".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleFinished("-----------".to_owned()),
				DownloadProgress::Skipped(1, SkippedType::InArchive),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
PARSE_START 'youtube' '-----------' Some Title Here
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '-----------'
[youtube] someId: has already been recorded in the archive
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("-----------", "youtube").with_title("Some Title Here")],
				media_vec
			);
		}

		/// Test to test skipping, erroring and normal download together
		#[test]
		fn test_skip_error_and_normal() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::PlaylistInfo(4), // "[] Playlist ...: Downloading ... items of ..."
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleFinished("someid4".to_owned()),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4
[download] Downloading item 1 of 4
[aprovider] someid1: has already been recorded in the archive
[download] Downloading item 2 of 4
[aprovider] someid2: has already been recorded in the archive
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://someurl.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
ERROR: [aprovider] someid3: somekinda error
[download] Downloading item 4 of 4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Some Title Here [someid4].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid4' /path/to/somewhere
PARSE_END 'aprovider' 'someid4'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("someid4", "aprovider")
						.with_title("Some Title Here")
						.with_filename("somewhere")
				],
				media_vec
			);
		}

		#[test]
		fn test_warning_line() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::SingleStarting("-----------".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleFinished("-----------".to_owned()),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
PARSE_START 'youtube' '-----------' Some Title Here
WARNING: [youtube] Falling back to generic n function search
         player = https://youtube.com/some.js
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '-----------'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("-----------", "youtube").with_title("Some Title Here")],
				media_vec
			);
		}

		/// Test that when a error happens while downloading that the media is not added as a final media
		#[test]
		fn test_error_while_downloading() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::PlaylistInfo(4), // "[] Playlist ...: Downloading ... items of ..."
				DownloadProgress::SingleStarting("someid1".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 100),
				DownloadProgress::SingleFinished("someid1".to_owned()),
				DownloadProgress::SingleStarting("someid2".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid2".to_owned()), 2),
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::SingleStarting("someid3".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid3".to_owned()), 0),
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4

[download] Downloading item 1 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid2
[aprovider] someid1: Downloading JSON metadata
[info] someid1: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid1' Some Title Here
[download] Destination: Some Title Here [someid1].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid1' /path/to/somewhere
PARSE_END 'aprovider' 'someid1'

[download] Downloading item 2 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid2
[aprovider] someid2: Downloading JSON metadata
[info] someid2: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid2' Some Title Here
[download] Destination: Happy Halloween Mona [someid2].mp4
[download]   2.7% of  5.00MiB at    4.18MiB/s ETA 01:09

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid2'
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
[info] someid3: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid3' Some Title Here
[download] Destination: Pjanoo Mona [someid3].mp4
[download]   0.0% of  6.00MiB at  Unknown B/s ETA Unknown

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid3'
[download] Downloading item 4 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Girls Mona [someid4].mp4
[download]   0.0% of  7.00MiB at  Unknown B/s ETA Unknown

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid4'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("someid1", "aprovider")
						.with_title("Some Title Here")
						.with_filename("somewhere")
				],
				media_vec
			);
		}

		/// Test parsing of "[] Playlist ...: Downloading ... items of ..." lines
		#[test]
		fn test_playlistsize_from_playlist_downloading_items() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::PlaylistInfo(4), // "[] Playlist ...: Downloading ... items of ..."
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleFinished("someid4".to_owned()),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4
[download] Downloading item 1 of 4
[aprovider] someid1: has already been recorded in the archive
[download] Downloading item 2 of 4
[aprovider] someid2: has already been recorded in the archive
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://someurl.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
ERROR: [aprovider] someid3: somekinda error
[download] Downloading item 4 of 4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Some Title Here [someid4].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid4' /path/to/somewhere
PARSE_END 'aprovider' 'someid4'
	"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("someid4", "aprovider")
						.with_title("Some Title Here")
						.with_filename("somewhere")
				],
				media_vec
			);
		}

		/// Test parsing of "PLAYLIST ''" lines
		#[test]
		fn test_playlistsize_from_custom_playlist() {
			let expected_pg = &vec![
				DownloadProgress::UrlStarting,
				DownloadProgress::PlaylistInfo(4), // "[] Playlist ...: Downloading ... items of ..."
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::InArchive), // one archive skip
				DownloadProgress::Skipped(1, SkippedType::Error), // one error skip
				DownloadProgress::PlaylistInfo(4), // custom "PLAYLIST ''" line
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleFinished("someid4".to_owned()),
				DownloadProgress::UrlFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4
[download] Downloading item 1 of 4
[aprovider] someid1: has already been recorded in the archive
[download] Downloading item 2 of 4
[aprovider] someid2: has already been recorded in the archive
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://someurl.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
ERROR: [aprovider] someid3: somekinda error
[download] Downloading item 4 of 4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PLAYLIST '4'
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Some Title Here [someid4].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid4' /path/to/somewhere
PARSE_END 'aprovider' 'someid4'
"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("someid4", "aprovider")
						.with_title("Some Title Here")
						.with_filename("somewhere")
				],
				media_vec
			);
		}
	}
}
