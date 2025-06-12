//! Module that contains all logic for spawning the "ytdl" command
use std::{
	process::{
		Command,
		Output,
		Stdio,
	},
	sync::LazyLock,
};

use regex::Regex;

use crate::error::IOErrorToError;

use super::ffmpeg::require_ffmpeg_installed;

/// Binary name to spawn for the youtube-dl process
pub const YTDL_BIN_NAME: &str = "yt-dlp";

/// Create a new [YTDL_BIN_NAME] [Command] instance
#[inline]
#[must_use]
pub fn base_ytdl() -> Command {
	return Command::new(YTDL_BIN_NAME);
}

/// Test if ytdl is installed and reachable, including required dependencies like ffmpeg and return the version found.
///
/// This function is not automatically called in the library, it is recommended to run this in any binary trying to run libytdlr.
pub fn require_ytdl_installed() -> Result<String, crate::Error> {
	require_ffmpeg_installed()?;

	return match ytdl_version() {
		Ok(v) => Ok(v),
		Err(err) => {
			log::error!("Could not start or find youtube-dl! Error: {}", err);

			return Err(crate::Error::custom_ioerror_location(
				std::io::ErrorKind::NotFound,
				"Youtube-DL(p) Version could not be determined, is it installed and reachable?",
				format!("{} in PATH", YTDL_BIN_NAME),
			));
		},
	};
}

/// Regex to parse the version from a "youtube-dl --version" output
/// cap1: version (date)
static YTDL_VERSION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
	return Regex::new(r"(?mi)^(\d{4}\.\d{1,2}\.\d{1,2})").unwrap();
});

/// Get Version of `ffmpeg`
#[inline]
pub fn ytdl_version() -> Result<String, crate::Error> {
	let mut cmd = base_ytdl();
	cmd.arg("--version");

	let command_output: Output = cmd
		.stderr(Stdio::null())
		.stdout(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()
		.attach_location_err("ytdl spawn")?
		.wait_with_output()
		.attach_location_err("ytdl wait_with_output")?;

	if !command_output.status.success() {
		return Err(crate::Error::command_unsuccessful("FFMPEG did not successfully exit!"));
	}

	let as_string = String::from_utf8(command_output.stdout)?;

	return ytdl_parse_version(&as_string);
}

/// Internal Function to parse the input to a ytdl version with regex
#[inline]
fn ytdl_parse_version(input: &str) -> Result<String, crate::Error> {
	return Ok(YTDL_VERSION_REGEX
		.captures_iter(input)
		.next()
		.ok_or_else(|| return crate::Error::no_captures("YTDL Version could not be determined"))?[1]
		.to_owned());
}

/// Try to parse a given `input`, which is a youtube-dl(p) version, as a [NaiveDate](chrono::NaiveDate).
pub fn ytdl_parse_version_naivedate(input: &str) -> Result<chrono::NaiveDate, crate::Error> {
	let version = ytdl_parse_version(input)?;

	let date = chrono::NaiveDate::parse_from_str(&version, "%Y.%m.%d").map_err(|err| {
		return crate::Error::other(format!("Could not parse \"{version}\" as a date: {err}"));
	})?;

	return Ok(date);
}

#[cfg(test)]
mod test {
	use chrono::NaiveDate;

	use crate::spawn::ytdl::ytdl_parse_version_naivedate;

	use super::ytdl_version;

	#[test]
	fn test_ytdl_parse_version_invalid_input() {
		assert_eq!(
			super::ytdl_parse_version("hello"),
			Err(crate::Error::no_captures("YTDL Version could not be determined"))
		);
	}

	#[test]
	fn test_ytdl_parse_version_valid_static_input() {
		let ytdl_output = "2021.12.27";

		assert_eq!(super::ytdl_parse_version(ytdl_output), Ok("2021.12.27".to_owned()));
	}

	#[test]
	#[ignore = "CI Install not present currently"]
	fn test_ytdl_spawn() {
		assert!(ytdl_version().is_ok());
	}

	#[test]
	fn test_parse_naivedate() {
		assert_eq!(
			NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
			ytdl_parse_version_naivedate("2024.01.01").unwrap()
		);
	}
}
