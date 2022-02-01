//! Module that contains all logic for spawning the "ytdl" command
use std::process::Command;
use std::process::{
	Output,
	Stdio,
};

use regex::Regex;

#[inline]
pub fn base_ytdl() -> Command {
	return super::multiplatform::spawn_command("youtube-dl");
}

lazy_static! {
	static ref YTDL_VERSION_REGEX: Regex = Regex::new(r"(?mi)^(\d{4}\.\d{1,2}\.\d{1,2})").unwrap();
}

/// Get Version of `ffmpeg`
#[inline]
pub fn ytdl_version() -> Result<String, crate::Error> {
	let mut cmd = base_ytdl();
	cmd.arg("--version");

	let command_output: Output = cmd
		.stderr(Stdio::null())
		.stdout(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()?
		.wait_with_output()?;

	if !command_output.status.success() {
		return Err(crate::Error::CommandNotSuccesfull(
			"FFMPEG did not successfully exit!".to_string(),
		));
	}

	let as_string = String::from_utf8(command_output.stdout)?;

	return ytdl_parse_version(&as_string);
}

/// Internal Function to parse the input to a ffmpeg version with regex
#[inline]
fn ytdl_parse_version(input: &str) -> Result<String, crate::Error> {
	return Ok(YTDL_VERSION_REGEX
		.captures_iter(&input)
		.next()
		.ok_or_else(|| return crate::Error::NoCapturesFound("YTDL Version could not be determined".to_owned()))?[1]
		.to_owned());
}

#[cfg(test)]
mod test {
	use super::ytdl_version;

	#[test]
	pub fn test_ytdl_parse_version_invalid_input() -> () {
		assert_eq!(
			super::ytdl_parse_version("hello"),
			Err(crate::Error::NoCapturesFound(
				"YTDL Version could not be determined".to_owned()
			))
		);
	}

	#[test]
	pub fn test_ytdl_parse_version_valid_static_input() -> () {
		let ytdl_output = "2021.12.27";

		assert_eq!(super::ytdl_parse_version(&ytdl_output), Ok("2021.12.27".to_owned()));
	}

	#[test]
	#[ignore = "CI Install not present currently"]
	pub fn test_ytdl_spawn() -> () {
		assert!(ytdl_version().is_ok());
	}
}
