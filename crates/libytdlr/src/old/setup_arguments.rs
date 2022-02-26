use super::archive_schema::Archive;
use super::setup_archive::setup_archive;
use super::utils::Arguments;

use std::fs::create_dir_all;
use std::io::{
	Error as ioError,
	Result as ioResult,
};
use std::path::Path;
use std::path::PathBuf;

/// Helper function to make code more clean
#[inline]
fn process_paths<T: AsRef<Path>>(val: T) -> ioResult<PathBuf> {
	return crate::utils::to_absolute(val);
}

/// Process input to useable PathBuf for temporary directory
fn get_tmp_path(val: Option<PathBuf>) -> ioResult<PathBuf> {
	let mut ret_path = process_paths({
		if let Some(path) = val {
			path
		} else {
			std::env::temp_dir()
		}
	})?;

	if ret_path.exists() && !ret_path.is_dir() {
		debug!("Temporary path exists, but is not an directory");
		ret_path.pop();
	}

	// its "3" because "/" is an ancestor and "tmp" is an ancestor
	if ret_path.ancestors().count() < 3 {
		debug!(
			"Adding another directory to YTDL_TMP, original: \"{}\"",
			ret_path.display()
		);
		ret_path = ret_path.join("ytdl-rust");

		create_dir_all(&ret_path)?;
	}

	return Ok(ret_path);
}

/// Process input to useable Archive
fn get_config_path(val: Option<PathBuf>) -> ioResult<Option<Archive>> {
	let archive_path = process_paths({
		if let Some(path) = val {
			path
		} else {
			dirs_next::config_dir()
				.expect("Could not find an Default Config Directory")
				.join("ytdl_archive.json")
		}
	})?;

	return Ok(setup_archive(archive_path));
}

/// Process input to useable PathBuf for Output
fn get_output_path(val: Option<PathBuf>) -> ioResult<PathBuf> {
	let mut ret_path = process_paths({
		if let Some(path) = val {
			path
		} else {
			dirs_next::download_dir()
				.unwrap_or_else(|| return PathBuf::from("."))
				.join("ytdl-out")
		}
	})?;

	if ret_path.exists() && !ret_path.is_dir() {
		debug!("Output path exists, but is not an directory");
		ret_path.pop();
	}

	return Ok(ret_path);
}

/// Wrapper for [`setup_args`] arguments
#[derive(Debug)]
pub struct SetupArgs {
	pub out:                  Option<PathBuf>,
	pub tmp:                  Option<PathBuf>,
	pub url:                  String,
	pub archive:              Option<PathBuf>,
	pub audio_only:           bool,
	pub debug:                bool,
	pub disable_re_thumbnail: bool,
	pub askedit:              bool,
	pub editor:               String,
}

/// Setup clap-arguments
pub fn setup_args(input: SetupArgs) -> Result<Arguments, ioError> {
	let mut args = Arguments {
		out:                  get_output_path(input.out)?,
		tmp:                  get_tmp_path(input.tmp)?,
		url:                  input.url,
		archive:              get_config_path(input.archive)?,
		audio_only:           input.audio_only,
		debug:                input.debug,
		disable_re_thumbnail: input.disable_re_thumbnail,
		askedit:              input.askedit,
		editor:               input.editor,
		extra_args:           Vec::new(),
	};

	if args.url.is_empty() {
		println!("URL is required!");
		std::process::exit(2);
	}

	args.extra_args.push("--write-thumbnail".to_owned());

	return Ok(args);
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	#[ignore]
	fn test_everything_default() {
		let arguments = setup_args(SetupArgs {
			archive:              None,
			askedit:              false,
			audio_only:           false,
			disable_re_thumbnail: false,
			debug:                false,
			editor:               String::from(""),
			url:                  "SomeURL".to_owned(),
			out:                  None,
			tmp:                  None,
		})
		.unwrap();

		let download_dir = dirs_next::download_dir().expect("Expected to have a downloaddir");

		assert_eq!(download_dir.join("ytdl-out"), arguments.out);

		assert_eq!(PathBuf::from("/tmp/ytdl-rust"), arguments.tmp);
		assert_eq!("SomeURL", arguments.url);
		assert_eq!(1, arguments.extra_args.len());
		assert!(!arguments.audio_only);
		assert!(!arguments.debug);
		assert!(!arguments.disable_re_thumbnail);
		assert!(arguments.archive.is_some());
		assert!(!arguments.askedit);
		assert!(arguments.editor.is_empty());
	}

	#[test]
	#[ignore = "somehow, updating to clap 3.x something here is not allowed anymore - ignoring until moving away from yaml"]
	fn test_arguments_tmp_add_ancestor() {
		let archive = setup_args(SetupArgs {
			out:                  None,
			tmp:                  Some(PathBuf::from("/tmp")),
			url:                  "SomeURL".to_owned(),
			archive:              None,
			audio_only:           false,
			debug:                false,
			disable_re_thumbnail: false,
			askedit:              false,
			editor:               String::from(""),
		})
		.unwrap();

		assert_eq!(PathBuf::from("/tmp/ytdl-rust"), archive.tmp);
	}
}
