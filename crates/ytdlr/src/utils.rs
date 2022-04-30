//! Utils for the `ytdlr` binary

use crate::clap_conf::*;
use diesel::SqliteConnection;
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
};
use libytdlr::{
	data::cache::media_info::MediaInfo,
	main::archive::import::ImportProgress,
	spawn::{
		ffmpeg::ffmpeg_version,
		ytdl::ytdl_version,
	},
};
use std::{
	borrow::Cow,
	ffi::OsStr,
	io::{
		BufRead,
		BufReader,
		Error as ioError,
		Write,
	},
	os::unix::prelude::ExitStatusExt,
	path::{
		Path,
		PathBuf,
	},
	process::Stdio,
};

/// Helper function to set the progressbar to a draw target if mode is interactive
pub fn set_progressbar(bar: &ProgressBar, main_args: &CliDerive) {
	if main_args.is_interactive() {
		bar.set_draw_target(ProgressDrawTarget::stderr());
	}
}

/// Test if Youtube-DL(p) is installed and reachable, including required dependencies like ffmpeg
pub fn require_ytdl_installed() -> Result<(), ioError> {
	require_ffmpeg_installed()?;

	if let Err(err) = ytdl_version() {
		log::error!("Could not start or find ytdl! Error: {}", err);

		return Err(ioError::new(
			std::io::ErrorKind::NotFound,
			"Youtube-DL(p) Version could not be determined, is it installed and reachable?",
		));
	}

	return Ok(());
}

/// Test if FFMPEG is installed and reachable
pub fn require_ffmpeg_installed() -> Result<(), ioError> {
	if let Err(err) = ffmpeg_version() {
		log::error!("Could not start or find ffmpeg! Error: {}", err);

		return Err(ioError::new(
			std::io::ErrorKind::NotFound,
			"FFmpeg Version could not be determined, is it installed and reachable?",
		));
	}

	return Ok(());
}

/// Generic handler function for using [`libytdlr::main::sql_utils::migrate_and_connect`] with a [`ProgressBar`]
pub fn handle_connect<'a>(
	archive_path: &'a Path,
	bar: &ProgressBar,
	main_args: &CliDerive,
) -> Result<(Cow<'a, Path>, SqliteConnection), libytdlr::Error> {
	let pgcb_migrate = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Migrating {} elements", v)),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Migration"),
				ImportProgress::SizeHint(v) => println!("Migration SizeHint: {}", v),
				ImportProgress::Increase(c, i) => println!("Migration Increase: {}, Current Index: {}", c, i),
				ImportProgress::Finished(v) => println!("Migration Finished, Successfull Migrations: {}", v),
				_ => (),
			}
		}
	};

	let res = libytdlr::main::sql_utils::migrate_and_connect(archive_path, pgcb_migrate)?;

	if res.0 != archive_path {
		bar.finish_with_message(format!(
			"Migration from JSON to SQLite archive done, Archive path has changed to \"{}\"",
			res.0.to_string_lossy()
		));
	} else {
		bar.finish_and_clear();
	}

	return Ok(res);
}

/// Find all files in the provided "path" that could be edited (like mkv, mp3)
pub fn find_editable_files<P: AsRef<Path>>(path: P) -> Result<Vec<MediaInfo>, crate::Error> {
	let path = path.as_ref();

	// some basic checks that the path is actually valid
	if !path.exists() {
		return Err(crate::Error::Other(format!(
			"Path for finding editable files does not exist! (Path: \"{}\")",
			path.to_string_lossy()
		)));
	}

	if !path.is_dir() {
		return Err(crate::Error::Other(format!(
			"Path for finding editable files is not a directory! (Path: \"{}\")",
			path.to_string_lossy()
		)));
	}

	let mut mediainfo_vec: Vec<MediaInfo> = Vec::default();

	// do a loop over each element in the directory, and filter out paths that are not valid / accessable
	for entry in std::fs::read_dir(path)? {
		if let Ok(entry) = entry {
			if let Some(mediainfo) = process_path_for_editable_files(entry.path()) {
				mediainfo_vec.push(mediainfo);
			}
		}
	}

	return Ok(mediainfo_vec);
}

/// Helper function to reduce nesting ofr [`find_editable_files`]
/// for example, in a loop "?" cannot be used, but in a helper function
#[inline]
fn process_path_for_editable_files(path: PathBuf) -> Option<MediaInfo> {
	// make sure that only files are filtered in
	if !path.is_file() {
		return None;
	}
	// make sure that only files with a supported extension are filtered in
	if !match_extension_for_editable_files(path.extension()?) {
		return None;
	}

	return MediaInfo::try_from_filename(&path.file_name()?.to_str()?);
}

// Array of AUDIO extensions supported for matching in ytdlr
const AUDIO_EXTENSION_LIST: &'static [&'static str] = &["mp3", "wav", "aac", "ogg"];
// Array of VIDEO extensions supported for matching in ytdlr
const VIDEO_EXTENSION_LIST: &'static [&'static str] = &["mp4", "mkv", "webm"];

/// Helper function to keep all extension matching for [`find_editable_files`] sorted
#[inline]
fn match_extension_for_editable_files<'a>(input: &'a OsStr) -> bool {
	// convert "input" to a str (from OsStr), and if not possible return "false"
	if let Some(input) = input.to_str() {
		if AUDIO_EXTENSION_LIST.contains(&input) | VIDEO_EXTENSION_LIST.contains(&input) {
			return true;
		}
	}

	return false;
}

/// Struct for [`get_filetype`] to easily differentiate between file formats
#[derive(Debug, PartialEq, Clone)]
pub enum FileType {
	/// Variant indicating that the filename that was tested is a Video Format
	Video,
	/// Variant indicating that the filename that was tested is a Audio Format
	Audio,
	/// Variant indicating that the filename that was tested could not be indentified
	Unknown,
}

/// Get what type the "path" is
pub fn get_filetype<F: AsRef<Path>>(filename: F) -> FileType {
	let filename = filename.as_ref();

	// only match extensions that can be a str
	if let Some(ext) = filename.extension().and_then(|v| return v.to_str()) {
		if AUDIO_EXTENSION_LIST.contains(&ext) {
			return FileType::Audio;
		}

		if VIDEO_EXTENSION_LIST.contains(&ext) {
			return FileType::Video;
		}
	}

	return FileType::Unknown;
}

/// Get input from STDIN with "possible" or "default"
/// if using "default", remember to set a character in "possible" to upper-case
pub fn get_input<'a>(msg: &'a str, possible: &[&'static str], default: &'static str) -> Result<String, crate::Error> {
	// TODO: maybe consider replacing this with the crate "dialoguer"
	let possible_converted = possible
		.iter()
		.map(|v| {
			return v.to_lowercase();
		})
		// the following is used, because ".join" is not valid for iterators
		// this may be inefficient
		.collect::<Vec<String>>();
	// dont use "possible_converted" for "possible_converted_string", because otherwise the default will not be shown anymore
	let possible_converted_string = possible.join("/");
	loop {
		print!("{} [{}]: ", msg, possible_converted_string);
		// ensure the message is printed before reading
		std::io::stdout().flush()?;
		// input buffer for "read_line", 1 capacity, because of only expecting 1 character
		let mut input = String::with_capacity(1);
		std::io::stdin().read_line(&mut input)?;

		let input = input.trim().to_lowercase();

		// return default if empty and default is set
		if input.is_empty() {
			if !default.is_empty() {
				return Ok(default.to_owned());
			} else {
				// special case when empty, to more emphasize that its empty
				println!("... Invalid Input: (Empty)");
				continue;
			}
		}

		if possible_converted.contains(&input) {
			return Ok(input.to_owned());
		}

		println!("... Invalid Input: \"{}\"", input);
	}
}

/// Run a editor with provided path and resolve not having a editor
/// `path` input is not checked to be a file or directory, so it should be checked beforehand
pub fn run_editor(maybe_editor: &Option<PathBuf>, path: &Path, print_editor_stdout: bool) -> Result<(), crate::Error> {
	if !path.exists() {
		return Err(ioError::new(
			std::io::ErrorKind::NotFound,
			format!("File to Edit does not exist! (Path: \"{}\")", path.to_string_lossy()),
		)
		.into());
	}

	let mut editor_child = {
		let mut cmd = libytdlr::spawn::editor::base_editor(&get_editor_base(maybe_editor)?, path);

		if print_editor_stdout {
			cmd.stdout(Stdio::piped());
		} else {
			cmd.stdout(Stdio::null());
		}

		cmd.stderr(Stdio::piped()).stdin(Stdio::null());

		cmd.spawn()
	}?;

	let stderr_reader = BufReader::new(
		editor_child
			.stderr
			.take()
			.ok_or_else(|| return crate::Error::Other("Failed to take Editor Child's STDERR".to_owned()))?,
	);

	let editor_child_stderr_thread = std::thread::Builder::new()
		.name("editor stderr handler".to_owned())
		.spawn(move || {
			stderr_reader
				.lines()
				.filter_map(|line| return line.ok())
				.for_each(|line| {
					info!("editor [STDERR]: \"{}\"", line);
				})
		})?;

	let mut editor_child_stdout_thread = None;

	// only create a stdout handler thread if needed
	if print_editor_stdout {
		let stdout_reader = BufReader::new(
			editor_child
				.stdout
				.take()
				.ok_or_else(|| return crate::Error::Other("Failed to take Editor Child's STDOUT".to_owned()))?,
		);

		editor_child_stdout_thread = Some(
			std::thread::Builder::new()
				.name("editor stdout handler".to_owned())
				.spawn(move || {
					stdout_reader
						.lines()
						.filter_map(|line| return line.ok())
						.for_each(|line| {
							trace!("editor [STDOUT]: \"{}\"", line);
						})
				})?,
		)
	}

	// wait until the editor_child has exited and get the status
	let editor_child_exit_status = editor_child.wait()?;

	editor_child_stderr_thread.join().map_err(|err| {
		return crate::Error::Other(format!("Joining the editor_child STDERR handle failed: {:?}", err));
	})?;

	if let Some(thread) = editor_child_stdout_thread {
		thread.join().map_err(|err| {
			return crate::Error::Other(format!("Joining the editor_child STDOUT handle failed: {:?}", err));
		})?;
	}

	if !editor_child_exit_status.success() {
		return Err(match editor_child_exit_status.code() {
			Some(code) => crate::Error::Other(format!("editor_child exited with code: {}", code)),
			None => {
				let signal = match editor_child_exit_status.signal() {
					Some(code) => code.to_string(),
					None => "None".to_owned(),
				};

				crate::Error::Other(format!("editor_child exited with signal: {}", signal))
			},
		});
	}

	return Ok(());
}

/// Try to get the editor from the input argument, if not ask the user to provide a path
fn get_editor_base(maybe_editor: &Option<PathBuf>) -> Result<PathBuf, crate::Error> {
	if let Some(editor) = maybe_editor {
		// return path if "Some", if none ask for another new path
		if let Some(path) = test_editor_base(&editor)? {
			return Ok(path);
		}
	}

	// path where "maybe_editor" is "none" or user selected to "set new path" because not existing
	'ask_for_editor: loop {
		print!("Enter new Editor base: ");
		// ensure the message is printed before reading
		std::io::stdout().flush()?;
		// input buffer for "read_line", 1 capacity, because of only expecting 1 character
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;

		let input = input.trim();

		// return default if empty and default is set
		if input.is_empty() {
			println!("Input was empty, please try again");
			continue 'ask_for_editor;
		}

		// return path if "Some", if none ask for another new path
		if let Some(path) = test_editor_base(Path::new(&input))? {
			return Ok(path);
		}
	}
}

/// Helper function for [`get_editor_base`] to test the path to be valid
/// Returns [`Ok(Some)`] if the path is valid and ok
/// Returns [`Ok(None)`] if a new path should be prompted
fn test_editor_base(path: &Path) -> Result<Option<PathBuf>, crate::Error> {
	'test_editor: loop {
		let test_result = test_editor_base_valid(&path);
		if test_result.is_ok() {
			return Ok(Some(path.to_owned()));
		}

		let err = test_result.expect_err("Expected \"if is_ok\" to return");

		println!("Editor base is not available, Error: {}", err);

		let input = get_input("[R]etry, [a]bort or [s]et new path?", &["R", "a", "s"], "r")?;

		match input.as_str() {
			"r" => continue 'test_editor,
			"a" => return Err(crate::Error::Other("Abort Selected".to_owned())),
			"s" => return Ok(None),
			_ => unreachable!("get_input should only return a OK value from the possible array"),
		}
	}
}

/// Helper function for [`get_editor_base`] to test the input argument if it is fully executeable
fn test_editor_base_valid(path: &Path) -> Result<(), ioError> {
	// this function currently does not much, but is here for future additions
	if path.as_os_str().is_empty() {
		return Err(ioError::new(std::io::ErrorKind::NotFound, "Editor base is empty!"));
	}

	return Ok(());
}

/// Convert a "MediaInfo" instance to a filename
/// Returns [`Some`] the final filename (Path Format: "title.extension") (filename, final_filename)
/// Returns [`None`] when `media.title` or `media.filename` or `media.filename.extension` are [`None`]
#[inline]
pub fn convert_mediainfo_to_filename<'a>(media: &'a MediaInfo) -> Option<(&'a PathBuf, PathBuf)> {
	let media_filename = media.filename.as_ref()?;
	let media_title = media.title.as_ref()?;
	let extension = media_filename.extension()?;

	return Some((media_filename, Path::new(media_title).with_extension(extension)));
}
