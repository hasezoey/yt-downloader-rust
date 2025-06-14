//! Utils for the `ytdlr` binary

use crate::{
	TERMINATE,
	clap_conf::CliDerive,
};
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
};
use libytdlr::{
	data::cache::media_info::MediaInfo,
	diesel::SqliteConnection,
	error::{
		CustomThreadJoin,
		IOErrorToError,
	},
	main::archive::import::ImportProgress,
};
use std::{
	borrow::Cow,
	ffi::{
		OsStr,
		OsString,
	},
	io::{
		Error as ioError,
		Write,
	},
	os::unix::{
		fs::MetadataExt,
		prelude::{
			ExitStatusExt,
			OsStrExt,
		},
	},
	path::{
		Path,
		PathBuf,
	},
	process::Stdio,
	sync::mpsc,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Helper function to set the progressbar to a draw target based on if it is interactive
pub fn set_progressbar(bar: &ProgressBar, main_args: &CliDerive) {
	if main_args.is_interactive() {
		bar.set_draw_target(ProgressDrawTarget::stderr());
	}
}

/// Handler function for using [`libytdlr::main::sql_utils::migrate_and_connect`] with a [`ProgressBar`]
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
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Migrating {v} elements")),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Migration"),
				ImportProgress::SizeHint(v) => println!("Migration SizeHint: {v}"),
				ImportProgress::Increase(c, i) => println!("Migration Increase: {c}, Current Index: {i}"),
				ImportProgress::Finished(v) => println!("Migration Finished, Successfull Migrations: {v}"),
			}
		}
	};

	let res = libytdlr::main::sql_utils::migrate_and_connect(archive_path, pgcb_migrate)?;

	bar.finish_and_clear();
	if res.0 != archive_path {
		bar.println(format!(
			"Migration from JSON to SQLite archive done, Archive path has changed to \"{}\"",
			res.0.to_string_lossy()
		));
	}

	return Ok(res);
}

/// Find all files in the provided "path" that could be edited (like mkv, mp3)
pub fn find_editable_files<P: AsRef<Path>>(path: P) -> Result<Vec<MediaInfo>, crate::Error> {
	let path = path.as_ref();

	// some basic checks that the path is actually valid
	if !path.exists() {
		return Err(crate::Error::other(format!(
			"Path for finding editable files does not exist! Path: \"{}\"",
			path.to_string_lossy()
		)));
	}

	if !path.is_dir() {
		return Err(crate::Error::not_a_directory(
			"Path for finding editable files is not a directory!",
			path,
		));
	}

	let mut mediainfo_vec: Vec<MediaInfo> = Vec::default();

	// do a loop over each element in the directory, and filter out paths that are not valid / accessable
	for entry in (std::fs::read_dir(path).attach_path_err(path)?).flatten() {
		if let Ok(metadata) = entry.metadata() {
			if !metadata.is_file() || metadata.size() == 0 {
				continue;
			}
		}

		if let Some(mediainfo) = process_path_for_editable_files(&entry.path()) {
			mediainfo_vec.push(mediainfo);
		}
	}

	return Ok(mediainfo_vec);
}

/// Helper function to reduce nesting for [`find_editable_files`]
/// for example, in a loop "?" cannot be used, but in a helper function
#[inline]
fn process_path_for_editable_files(path: &Path) -> Option<MediaInfo> {
	// make sure that only files are filtered in
	if !path.is_file() {
		return None;
	}
	// make sure that only files with a supported extension are filtered in
	if !match_extension_for_editable_files(path.extension()?) {
		return None;
	}
	// if the name contains "temp" as a extension at the end, ignore the file
	// as it is not post-processed yet
	// example: "'provider'-'id'-some name.with.dots.temp.opus"
	if path
		.file_stem()
		.and_then(|v| return Path::new(v).extension())
		.is_some_and(|v| return v == "temp")
	{
		return None;
	}

	return MediaInfo::try_from_filename(&path.file_name()?.to_str()?);
}

// Array of AUDIO extensions supported for matching in ytdlr
const AUDIO_EXTENSION_LIST: &[&str] = &["mp3", "wav", "aac", "ogg", "opus", "m4a", "flac"];
// Array of VIDEO extensions supported for matching in ytdlr
const VIDEO_EXTENSION_LIST: &[&str] = &["mp4", "mkv", "webm"];

/// Helper function to keep all extension matching for [`find_editable_files`] sorted
#[inline]
fn match_extension_for_editable_files(input: &OsStr) -> bool {
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
///
/// Note:
/// This function will not discard buffered stdin, because in native rust there is no good way to clear a sync-read, and for async-read the whole library would be needed to be converted to async
// TODO: consider refactoring this to return some kind of struct which implements a trait
#[allow(clippy::needless_collect)] // this is because of a known false-positive https://github.com/rust-lang/rust-clippy/issues/6164
pub fn get_input(msg: &str, possible: &[&'static str], default: &'static str) -> Result<String, crate::Error> {
	// TODO: maybe consider replacing this with the crate "dialoguer"
	// ^ blocked https://github.com/console-rs/dialoguer/issues/248 & https://github.com/console-rs/dialoguer/issues/247
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
		print!("{msg} [{possible_converted_string}]: ");
		// ensure the message is printed before reading
		std::io::stdout().flush().attach_location_err("stdout flush")?;
		let input: String;

		// the following has to be done because "read_line" is blocking, but the ctrlc handler should still be able to work
		{
			let (tx, rx) = mpsc::channel::<Result<String, ioError>>();
			let read_thread = std::thread::Builder::new()
				.name("input reader".to_owned())
				.spawn(move || {
					// input buffer for "read_line", 1 capacity, because of only expecting 1 character
					let mut input = String::with_capacity(1);
					let _ = tx.send(std::io::stdin().read_line(&mut input).map(|_| return input));
				})
				.attach_location_err("input reader thread spawn")?;

			loop {
				// handle terminate
				if crate::TERMINATE
					.read()
					.map_err(|err| return crate::Error::other(format!("{err}")))?
					.termination_requested()
				{
					return Err(crate::Error::other("Termination Requested"));
				}

				match rx.try_recv() {
					Ok(v) => {
						input = v.attach_location_err("input reader line")?;
						break;
					},
					Err(mpsc::TryRecvError::Empty) => (),
					Err(mpsc::TryRecvError::Disconnected) => {
						return Err(crate::Error::other("Channel unexpectedly disconnected"));
					},
				}

				std::thread::sleep(std::time::Duration::from_millis(50)); // sleep 50ms to not immediately try again, but still be responding
			}

			read_thread.join_err()?;
		}

		let input = input.trim().to_lowercase();

		// return default if empty and default is set
		if input.is_empty() {
			if default.is_empty() {
				// special case when empty, to more emphasize that its empty
				println!("... Invalid Input: (Empty)");
				continue;
			} else {
				return Ok(default.to_owned());
			}
		}

		if possible_converted.contains(&input) {
			return Ok(input);
		}

		println!("... Invalid Input: \"{input}\"");
	}
}

/// Run a editor with provided path and resolve not having a editor
/// `path` input is not checked to be a file or directory, so it should be checked beforehand
pub fn run_editor(maybe_editor: Option<&Path>, path: &Path) -> Result<(), crate::Error> {
	if !path.exists() {
		return Err(crate::Error::custom_ioerror_path(
			std::io::ErrorKind::NotFound,
			"File to Edit does not exist!",
			path,
		));
	}

	let mut editor_child = {
		let mut cmd = libytdlr::spawn::editor::base_editor(&get_editor_base(maybe_editor)?, path);

		cmd.stderr(Stdio::inherit())
			.stdout(Stdio::inherit())
			.stdin(Stdio::inherit());

		debug!("Spawning Command with inherited STDIO");

		// disable the termination handler, because we have spawned a command which inherits STDIO and handles the signals
		TERMINATE
			.write()
			.expect("Expected TERMINATE handler to not be poisoned")
			.disable();

		cmd.spawn().attach_location_err("editor spawn")?
	};

	// wait until the editor_child has exited and get the status
	let editor_child_exit_status = editor_child.wait().attach_location_err("editor wait")?;

	TERMINATE
		.write()
		.expect("Expected TERMINATE handler to not be poisoned")
		.enable();

	if !editor_child_exit_status.success() {
		if let Some(code) = editor_child_exit_status.code() {
			info!("Command exited with status-code {code}");
		} else {
			let signal = match editor_child_exit_status.signal() {
				Some(code) => code.to_string(),
				None => "None".to_owned(),
			};

			info!("Command exited with signal {signal}");
		}
	}

	return Ok(());
}

/// Try to get the editor from the input argument, if not ask the user to provide a path
fn get_editor_base(maybe_editor: Option<&Path>) -> Result<PathBuf, crate::Error> {
	if let Some(editor) = maybe_editor {
		// return path if "Some", if none ask for another new path
		if let Some(path) = test_editor_base(editor)? {
			return Ok(path);
		}
	}

	// path where "maybe_editor" is "none" or user selected to "set new path" because not existing
	'ask_for_editor: loop {
		print!("Enter new Editor base: ");
		// ensure the message is printed before reading
		std::io::stdout().flush().attach_location_err("stdout flush")?;
		// input buffer for "read_line", 1 capacity, because of only expecting 1 character
		let mut input = String::new();
		std::io::stdin()
			.read_line(&mut input)
			.attach_location_err("stdin read")?;

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
		let err = match test_editor_base_valid(path) {
			Ok(()) => return Ok(Some(path.to_owned())),
			Err(err) => err,
		};

		println!("Editor base is not available, Error: {err}");

		let input = get_input("[R]etry, [a]bort or [s]et new path?", &["R", "a", "s"], "r")?;

		match input.as_str() {
			"r" => continue 'test_editor,
			"a" => return Err(crate::Error::other("Abort Selected")),
			"s" => return Ok(None),
			_ => unreachable!("get_input should only return a OK value from the possible array"),
		}
	}
}

/// Helper function for [`get_editor_base`] to test the input argument if it is fully executeable
fn test_editor_base_valid(path: &Path) -> Result<(), crate::Error> {
	// this function currently does not much, but is here for future additions
	if path.as_os_str().is_empty() {
		return Err(crate::Error::custom_ioerror_path(
			std::io::ErrorKind::NotFound,
			"Editor base is empty!",
			path,
		));
	}

	return Ok(());
}

/// Convert a "MediaInfo" instance to a filename
/// filename gets truncated to be below 255 bytes
/// Returns [`Some`] the final filename (Path Format: "title.extension") (filename, final_filename)
/// Returns [`None`] when `media.title` or `media.filename` or `media.filename.extension` are [`None`]
#[inline]
pub fn convert_mediainfo_to_filename(media: &MediaInfo) -> Option<(&PathBuf, PathBuf)> {
	let media_filename = media.filename.as_ref()?;
	let media_title = media.title.as_ref()?;
	let extension = media_filename.extension()?;

	// replace all "/" with a similar looking character, so to not create multiple segments
	let media_title_conv = media_title.replace('/', "⧸");

	let extension_length = extension.as_bytes().len() + 1;

	// the title to use in the end
	// using 254 instead of 255 just to be safe
	let title_use = if media_title_conv.len() + extension_length > 254 {
		let truncate_to_max = 254 - extension_length;
		truncate_to_size_bytes(&media_title_conv, truncate_to_max, true)
	} else {
		media_title_conv[..].into()
	};

	// convert converted title into OsString and add the extension
	// this needs to be done so that titles containing "." do not accidentally get overwritten by "set_extension"
	let mut final_name_osstr: OsString = title_use.as_ref().into();

	final_name_osstr.push(".");
	final_name_osstr.push(extension); // the extension can be easily added here, because we can safely assume the title does not have a extension

	return Some((media_filename, PathBuf::from(&final_name_osstr)));
}

/// Apply all required processing to paths that need extra processing
/// Returns [`None`] if any of the functions processing the input return [`None`] (which happens if they cannot fix the paths)
/// Returns [`Some`] with the fixed path
#[inline]
pub fn fix_path<P: AsRef<Path>>(ip: P) -> Option<PathBuf> {
	// currently there is only one process to be done
	return libytdlr::utils::expand_tidle(ip);
}

/// Helper struct for [msg_to_cluster] instead of having to use a tuple with unnamed fields
#[derive(Debug, PartialEq)]
pub struct CharInfo<'a> {
	/// Index of character in the characters vec
	pub start_index:      usize,
	/// Bytes length of the character
	pub byte_length:      usize,
	/// Display position
	pub display_pos:      usize,
	/// Bytes position of the full characters (including length)
	pub size_bytes_total: usize,
	/// The full character itself
	pub full_char:        &'a str,
}

/// Convert a given string into a array of [CharInfo] to index at the correct positions
pub fn msg_to_cluster<M>(msg: &M) -> Vec<CharInfo>
where
	M: AsRef<str>,
{
	let msg = msg.as_ref();

	let mut display_position = 0; // keep track of the actual displayed position
	let mut size_bytes_to = 0; // keep track of how much bytes all the previous characters plus the current take

	return msg
		.grapheme_indices(true)
		.map(|(i, s)| {
			display_position += s.width();

			#[cfg(feature = "workaround_fe0f")]
			if s.contains('\u{FE0F}') {
				display_position += 1;
			}

			size_bytes_to += s.len();
			return CharInfo {
				start_index:      i,
				byte_length:      s.len(),
				display_pos:      display_position,
				size_bytes_total: size_bytes_to,
				full_char:        s,
			};
		})
		.collect::<Vec<CharInfo>>();
}

/// Truncate a given message to be of max "to_size_bytes" bytes long
/// does not truncate if "msg" is less or equal to "to_size_bytes"
/// also replaces the last 3 characters (after truncation) with "..." to indicate a truncation if "replace_with_dot" is true
pub fn truncate_to_size_bytes<M>(msg: &M, to_size_bytes: usize, replace_with_dot: bool) -> Cow<str>
where
	M: AsRef<str>,
{
	let msg = msg.as_ref();

	// dont run function if size is lower or equal to target
	if msg.len() <= to_size_bytes {
		return msg.into();
	}

	// get all characters and their boundaries
	let characters = msg_to_cluster(&msg);

	// deduct the replacing "..." from the bytes, to not have to loop later again
	let stop_bytes = if replace_with_dot {
		to_size_bytes - 3
	} else {
		to_size_bytes
	};

	// cache ".len" because it does not need to be executed often
	let characters_len = characters.len();

	// index to truncate the message to
	// finds the first index where the "size_bytes_to" is equal or lower than "stop_bytes", from the back
	let characters_end_idx = characters
		.iter()
		.rev()
		.position(|charinfo| return charinfo.size_bytes_total <= stop_bytes)
		.map(|v| return characters_len - v); // substract "v" because ".rev().position()" counts *encountered elements* instead of actual index

	// get the char boundary for the last character's end
	let msg_end_idx = if let Some(characters_end_idx) = characters_end_idx {
		let charinfo = &characters[characters_end_idx - 1];
		charinfo.start_index + charinfo.byte_length
	} else {
		0
	};

	let mut ret = String::from(&msg[0..msg_end_idx]);

	if replace_with_dot {
		ret.push_str("...");
	}

	// a safety check to not return bad strings
	assert!(ret.len() <= to_size_bytes);

	return ret.into();
}

/// Truncate a given message to be of max "to_display_pos" display width long
/// does not truncate if "msg" is less or equal to "to_display_pos"
/// also replaces the last 3 characters (after truncation) with "..." to indicate a truncation if "replace_with_dot" is true
pub fn truncate_message_display_pos<M>(msg: &M, to_display_pos: usize, replace_with_dot: bool) -> Cow<str>
where
	M: AsRef<str>,
{
	let msg = msg.as_ref();

	// get all characters and their boundaries
	let (characters, characters_highest_display) = {
		let chars = msg_to_cluster(&msg);
		let dis_pos = chars[chars.len() - 1].display_pos;
		(chars, dis_pos)
	};

	// dont run function if size is lower or equal to target
	if characters_highest_display <= to_display_pos {
		return msg.into();
	}

	// deduct the replacing "..." from the display position, to not have to loop later again
	let stop_display_pos = if replace_with_dot {
		to_display_pos - 3
	} else {
		to_display_pos
	};

	// cache ".len" because it does not need to be executed often
	let characters_len = characters.len();

	// index to truncate the message to
	// finds the first index where the "display_pos" is equal or lower than "stop_display_pos", from the back
	let characters_end_idx = characters
		.iter()
		.rev()
		.position(|charinfo| return charinfo.display_pos <= stop_display_pos)
		.map(|v| return characters_len - v); // substract "v" because ".rev().position()" counts *encountered elements* instead of actual index

	// get the char boundary for the last character's end
	let msg_end_idx = if let Some(characters_end_idx) = characters_end_idx {
		let charinfo = &characters[characters_end_idx - 1];
		charinfo.start_index + charinfo.byte_length
	} else {
		0
	};

	let mut ret = String::from(&msg[0..msg_end_idx]);

	if replace_with_dot {
		ret.push_str("...");
	}

	return ret.into();
}

#[cfg(test)]
mod test {
	use super::*;

	mod truncate_to_size_bytes {
		use super::*;

		#[test]
		fn should_not_truncate_message() {
			let message = "hello";

			assert_eq!(message, truncate_to_size_bytes(&message, 100, true));
			assert_eq!(message, truncate_to_size_bytes(&message, 100, false));
		}

		#[test]
		fn should_truncate_latin_message() {
			let message = "hello there";

			assert_eq!("hello t...", truncate_to_size_bytes(&message, message.len() - 1, true));
			assert_eq!("hello ther", truncate_to_size_bytes(&message, message.len() - 1, false));
		}

		#[test]
		fn should_properly_truncate_at_unicode_boundary() {
			let message = "a…b…c"; // bytes: 1 + 3 + 1 + 3 + 1 = 9

			assert_eq!("a…b…", truncate_to_size_bytes(&message, message.len() - 1, false));
			assert_eq!("a…b", truncate_to_size_bytes(&message, message.len() - 2, false));

			assert_eq!("a…b...", truncate_to_size_bytes(&message, message.len() - 1, true));
			assert_eq!("a…...", truncate_to_size_bytes(&message, message.len() - 2, true));
		}
	}

	mod truncate_message_display_pos {
		use super::*;

		#[test]
		fn should_not_truncate_message() {
			let message = "hello";

			assert_eq!(message, truncate_message_display_pos(&message, 100, true));
			assert_eq!(message, truncate_message_display_pos(&message, 100, false));
		}

		#[test]
		fn should_truncate_latin_message() {
			let message = "hello there"; // fully ascii, so len is also the display position

			assert_eq!(
				"hello t...",
				truncate_message_display_pos(&message, message.len() - 1, true)
			);
			assert_eq!(
				"hello ther",
				truncate_message_display_pos(&message, message.len() - 1, false)
			);
		}

		#[test]
		fn should_properly_truncate_at_unicode_boundary() {
			let message = "a…b…c"; // "…" is 3 bytes long, but displays as 1 character

			assert_eq!("a…b…", truncate_message_display_pos(&message, 4, false));
			assert_eq!("a…b", truncate_message_display_pos(&message, 3, false));

			assert_eq!("a...", truncate_message_display_pos(&message, 4, true));
			assert_eq!("...", truncate_message_display_pos(&message, 3, true));
		}
	}
}
