//! Module for counting a url output

use std::{
	io::{
		BufRead,
		BufReader,
	},
	process::Stdio,
	thread,
};

/// A Video that is returned from [`count`]
#[derive(Debug, PartialEq)]
pub struct CountVideo {
	/// The ID of the video / track
	pub id:    String,
	/// The Title of the Track
	pub title: String,
}

impl CountVideo {
	/// Create a new instance of [`CountVideo`]
	pub fn new(id: String, title: String) -> Self {
		return Self { id, title };
	}
}

// Implemented for easy casting, if needed
impl From<CountVideo> for crate::data::cache::media_info::MediaInfo {
	fn from(val: CountVideo) -> Self {
		return Self::new(val.id).with_title(val.title);
	}
}

/// Spawn ytdl and parse the output into a collection of [`CountVideo`]
/// Wrapper for [`count_with_command`] with [`crate::spawn::ytdl::base_ytdl`]
pub fn count<T: AsRef<str>>(url: T) -> Result<Vec<CountVideo>, crate::Error> {
	let mut cmd = crate::spawn::ytdl::base_ytdl();
	cmd.args(["-s", "--flat-playlist", "--get-id", "--get-title", url.as_ref()]);
	return count_with_command(cmd);
}

/// Spawn the `cmd` and parse the output into a collection of [`CountVideo`]
///
/// This function should not be used directly, use [`count`] instead
pub fn count_with_command(mut cmd: std::process::Command) -> Result<Vec<CountVideo>, crate::Error> {
	let mut videos: Vec<CountVideo> = Vec::new();

	// create a command and spawn it
	let mut child = {
		cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());

		cmd.spawn()?
	};

	let stdout_reader = BufReader::new(child.stdout.take().ok_or_else(|| {
		return crate::Error::IoError(std::io::Error::new(
			std::io::ErrorKind::BrokenPipe,
			"Failed to get Child STDOUT",
		));
	})?);

	let stderr_reader = BufReader::new(child.stderr.take().ok_or_else(|| {
		return crate::Error::IoError(std::io::Error::new(
			std::io::ErrorKind::BrokenPipe,
			"Failed to get Child STDERR",
		));
	})?);

	// offload the stderr reader to a different thread to not block main
	let stderrreader_thread = thread::spawn(|| {
		stderr_reader
			.lines()
			.filter_map(|v| return v.ok())
			.for_each(|line| log::info!("ytdl STDERR: {}", line));
	});

	// temporary storage for the video title
	let mut tmp_title = Option::from(String::new());

	// manually keeping track of the index, because of ignoring some lines (like empty lines)
	let mut index = 0usize;

	stdout_reader.lines().try_for_each(|line| -> Result<(), crate::Error> {
		let line = match line.ok() {
			Some(v) => v,
			None => return Ok(()),
		};

		// ignore empty lines, like the last line being a EOF
		if line.is_empty() {
			return Ok(());
		}

		index += 1;

		// ytdl(p) line format is:
		// odd: title
		// even: id
		//
		// this cannot be easily parsed to be correct, so we have to just assume that it is odd/even

		// if line is odd
		if (index % 2) != 0 {
			tmp_title = Some(line);
			return Ok(());
		}
		// if line is even
		let title = tmp_title
			.take()
			.ok_or_else(|| return crate::Error::Other("Expected a title, but was NONE".to_owned()))?;
		videos.push(CountVideo::new(line, title));

		return Ok(());
	})?;

	stderrreader_thread.join().expect("STDERR Reader Thread Join Failed");

	let exit_status = child.wait()?;

	if !exit_status.success() {
		return Err(crate::Error::IoError(std::io::Error::new(
			std::io::ErrorKind::Other,
			format!("Child Process for counting did not successfully exit: {}", exit_status),
		)));
	}

	if tmp_title.is_some() {
		return Err(crate::Error::Other(
			"Expected to not have a residual title, did ytdl mess up?".to_owned(),
		));
	}

	return Ok(videos);
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_countvideo_new() {
		assert_eq!(
			CountVideo {
				id:    "HelloId".to_owned(),
				title: "HelloTitle".to_owned(),
			},
			CountVideo::new("HelloId".to_owned(), "HelloTitle".to_owned())
		);
	}

	#[test]
	fn test_countvideo_into_mediainfo() {
		use crate::data::cache::media_info::MediaInfo;

		assert_eq!(
			MediaInfo::new("helloId1").with_title("HelloTitle1"),
			CountVideo::new("helloId1".to_owned(), "HelloTitle1".to_owned()).into()
		);
	}

	#[test]
	fn test_basic_func() {
		let mut fake_command = std::process::Command::new("echo");
		fake_command.args([
			// "-e" to explicitly enable escape-sequences
			"-e",
			// concat all together, because otherwise "echo" would add a leading space to each argument
			&["SomeTitle1\n", "someid1\n", "SomeTitle2\n", "someid2\n"].concat(),
		]);

		let output = count_with_command(fake_command);

		assert!(output.is_ok());

		assert_eq!(
			vec![
				CountVideo::new("someid1".to_owned(), "SomeTitle1".to_owned()),
				CountVideo::new("someid2".to_owned(), "SomeTitle2".to_owned())
			],
			output.expect("Expected Assert to test Result to be OK")
		);
	}

	#[test]
	fn test_err_residual_title() {
		let mut fake_command = std::process::Command::new("echo");
		fake_command.args([
			// "-e" to explicitly enable escape-sequences
			"-e",
			// concat all together, because otherwise "echo" would add a leading space to each argument
			&["SomeTitle1\n", "someid1\n", "SomeTitle2\n"].concat(),
		]);

		let output = count_with_command(fake_command);

		assert!(output.is_err());

		assert_eq!(
			crate::Error::Other("Expected to not have a residual title, did ytdl mess up?".to_owned()),
			output.expect_err("Expected Assert to test Result to be ERR")
		);
	}

	#[test]
	fn test_err_exit_status() {
		let mut fake_command = std::process::Command::new("sh");
		fake_command.args([
			"-c", // random exit code that is non-0
			"exit 1",
		]);

		let output = count_with_command(fake_command);

		assert!(output.is_err());

		assert_eq!(
			crate::Error::IoError(std::io::Error::new(
				std::io::ErrorKind::Other,
				"Child Process for counting did not successfully exit: 1".to_owned(),
			)),
			output.expect_err("Expected Assert to test Result to be ERR")
		);
	}
}
