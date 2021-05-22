use super::archive_schema::Archive;
use super::paths::to_absolute;
use super::setup_archive::setup_archive;
use super::utils::Arguments;

use std::ffi::OsStr;
use std::fs::create_dir_all;
use std::io::{
	Error as ioError,
	Result as ioResult,
};
use std::path::Path;
use std::path::PathBuf;

/// Helper function to make code more clean
#[inline]
fn string_to_bool(input: &str) -> bool {
	return matches!(input, "true");
}

/// Helper function to make code more clean
#[inline]
fn process_paths<T: AsRef<Path>>(val: T) -> ioResult<PathBuf> {
	return to_absolute(std::env::current_dir()?.as_path(), &val.as_ref());
}

fn get_tmp_path(val: Option<&OsStr>) -> ioResult<PathBuf> {
	let mut ret_path = process_paths({
		if let Some(path) = val {
			PathBuf::from(path)
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

fn get_config_path(val: Option<&OsStr>) -> ioResult<Option<Archive>> {
	let archive_path = process_paths({
		if let Some(path) = val {
			PathBuf::from(path)
		} else {
			dirs_next::config_dir()
				.expect("Could not find an Default Config Directory")
				.join("ytdl_archive.json")
		}
	})?;

	return Ok(setup_archive(archive_path));
}

/// Setup clap-arguments
pub fn setup_args(cli_matches: &clap::ArgMatches) -> Result<Arguments, ioError> {
	let mut args = Arguments {
		out:             process_paths(cli_matches.value_of_os("out").unwrap())?, // unwrap, because of a set default
		tmp:             get_tmp_path(cli_matches.value_of_os("tmp"))?,           // unwrap, because of a set default
		url:             cli_matches.value_of("URL").unwrap_or("").to_owned(),    // unwrap, because "URL" is required
		archive:         get_config_path(cli_matches.value_of_os("archive"))?,    // unwrap, because of a set default
		audio_only:      cli_matches.is_present("audio_only"),
		debug:           cli_matches.is_present("debug"),
		disable_cleanup: cli_matches.is_present("disablecleanup"),
		d_e_thumbnail:   cli_matches.is_present("disableeditorthumbnail"),
		askedit:         string_to_bool(cli_matches.value_of("askedit").unwrap()),
		editor:          cli_matches.value_of("editor").unwrap().to_owned(),
		extra_args:      cli_matches
			.values_of("ytdlargs") // get all values after "--"
			.map(|v| return v.collect::<Vec<&str>>()) // because "clap::Values" is an iterator, collect it all as Vec<&str>
			.unwrap_or_default() // unwrap the Option<Vec<&str>> or create a new Vec
			.iter() // Convert the Vec<&str> to an iterator
			.map(|v| return String::from(*v)) // Map every value to String (de-referencing because otherwise it would be "&&str")
			.collect(), // Collect it again as Vec<String>
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
	// TODO: Enable this test if discussion is resolved
	#[ignore = "https://github.com/clap-rs/clap/discussions/2489"]
	fn test_everything_default() {
		let args = vec!["bin", "SomeURL"];
		let yml = clap::load_yaml!("../cli.yml");
		let cli_matches = clap::App::from_yaml(yml).get_matches_from(args);

		let arguments = setup_args(&cli_matches).unwrap();

		assert_eq!(PathBuf::from("~/Downloads/ytdl-out"), arguments.out);
		assert_eq!(PathBuf::from("/tmp/ytdl-rust"), arguments.tmp);
		assert_eq!("SomeURL", arguments.url);
		assert!(arguments.extra_args.is_empty());
		assert!(!arguments.audio_only);
		assert!(!arguments.debug);
		assert!(!arguments.disable_cleanup);
		assert!(!arguments.d_e_thumbnail);
		assert!(arguments.archive.is_some());
		assert!(arguments.askedit);
		assert!(arguments.editor.is_empty());
	}

	#[test]
	fn test_arguments_tmp_add_ancestor() {
		let args = vec!["bin", "--tmp", "/tmp", "SomeURL"];
		let yml = clap::load_yaml!("../cli.yml");
		let cli_matches = clap::App::from_yaml(yml).get_matches_from(args);

		let archive = setup_args(&cli_matches).unwrap();

		assert_eq!(PathBuf::from("/tmp/ytdl-rust"), archive.tmp);
	}
}
