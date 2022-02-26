//! Module for Re-Applying Thumbnails to media

use std::{
	ffi::OsString,
	io::{
		BufRead,
		BufReader,
	},
	path::Path,
	process::Stdio,
	thread,
};

/// Re-Apply a thumbnail from `image` onto `media` as `output`
/// Where the output is added with a "tmp" to the `output` until finished
pub fn re_thumbnail_with_tmp<M: AsRef<Path>, I: AsRef<Path>, O: AsRef<Path>>(
	media: M,
	image: I,
	output: O,
) -> Result<(), crate::Error> {
	let mut output_path_tmp = output.as_ref().to_owned();

	// Generate a temporary filename, while leaving everything else like it was before
	{
		let mut stem = output_path_tmp
			.file_stem()
			.expect("Expected Output to be a file with name")
			.to_os_string();

		stem.push("_"); // add "_" to seperate the original name with the temporary one
		stem.push(std::process::id().to_string()); // add the current pid, so multiple instances can run at the same time

		stem.push(
			output_path_tmp
				.extension()
				// map extension to a extension with "."
				.map(|v| {
					let mut tmp = OsString::from(".");

					tmp.push(v);

					return tmp;
				})
				.unwrap_or_else(|| return OsString::from("")),
		); // push original extension, because there is currently no function to just modify the file stem

		output_path_tmp.set_file_name(stem);
	}

	re_thumbnail(media, image, &output_path_tmp)?;

	std::fs::rename(output_path_tmp, output.as_ref())?;

	return Ok(());
}

/// Re-Apply a thumbnail from `image` onto `media` as `output`
///
/// To Automatically handle with a temporary file, use [`re_thumbnail_with_tmp`]
pub fn re_thumbnail<M: AsRef<Path>, I: AsRef<Path>, O: AsRef<Path>>(
	media: M,
	image: I,
	output: O,
) -> Result<(), crate::Error> {
	let cmd = crate::spawn::ffmpeg::base_ffmpeg_hidebanner(true);

	return re_thumbnail_with_command(cmd, media, image, output);
}

/// Re-Apply a thumbnail from `image` onto `media` as `output` with base command `cmd`
///
/// This function should not be called directly, use [`re_thumbnail`] instead
pub fn re_thumbnail_with_command<M: AsRef<Path>, I: AsRef<Path>, O: AsRef<Path>>(
	mut cmd: std::process::Command,
	media: M,
	image: I,
	output: O,
) -> Result<(), crate::Error> {
	let media = media.as_ref();
	let image = image.as_ref();
	let output = output.as_ref();
	log::debug!(
		"ReThumbnail media \"{}\", with image \"{}\", into \"{}\"",
		media.to_string_lossy(),
		image.to_string_lossy(),
		output.to_string_lossy()
	);

	let mut child = {
		cmd.arg("-i").arg(media); // set media file as input "0"
		cmd.arg("-i").arg(image); // set image file as input "1"
		cmd.args([
			"-map",
			"0:0", // map input stream 0 to output stream 0
			"-map",
			"1:0", // map input stream 1 to output stream 0
			"-c",
			"copy", // copy all input streams into output stream without re-encoding
			"-id3v2_version",
			"3", // set which id3 version to use
			"-metadata:s:v",
			"title=\"Album cover\"", // set metadata for output video stream
			"-movflags",
			"use_metadata_tags", // copy existing metadata tags
		]);
		cmd.arg(output); // set output path

		// create pipe for stderr, other stream are ignored
		// this is because ffmpeg only logs to stderr, where stdout is used for data piping
		cmd.stdout(Stdio::null()).stderr(Stdio::piped()).stdin(Stdio::null());

		cmd.spawn()?
	};

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
			.for_each(|line| log::info!("ffmpeg STDERR: {}", line));
	});

	stderrreader_thread.join().expect("STDERR Reader Thread Join Failed");

	let exit_status = child.wait()?;

	if !exit_status.success() {
		return Err(crate::Error::IoError(std::io::Error::new(
			std::io::ErrorKind::Other,
			format!("ffmpeg did not successfully exit: {}", exit_status),
		)));
	}

	return Ok(());
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_basic_func() {
		let fake_command = std::process::Command::new("echo");

		let media = Path::new("/hello/media.mp3");
		let image = Path::new("/hello/image.jpg");
		let output = Path::new("/hello/output.mp3");

		let result = re_thumbnail_with_command(fake_command, media, image, output);

		assert!(result.is_ok());
	}

	#[test]
	fn test_exit_status() {
		let mut fake_command = std::process::Command::new("sh");
		fake_command.args([
			"-c", // random exit code that is non-0
			"exit 1",
		]);

		let media = Path::new("/hello/media.mp3");
		let image = Path::new("/hello/image.jpg");
		let output = Path::new("/hello/output.mp3");

		let output = re_thumbnail_with_command(fake_command, media, image, output);

		assert!(output.is_err());

		assert_eq!(
			crate::Error::IoError(std::io::Error::new(
				std::io::ErrorKind::Other,
				"ffmpeg did not successfully exit: 1".to_owned(),
			)),
			output.expect_err("Expected Assert to test Result to be ERR")
		);
	}
}
