//! Module that contains all logic for spawning the "ffmpeg" command
use std::{
	ffi::OsStr,
	os::unix::process::ExitStatusExt,
	process::{
		Command,
		Output,
		Stdio,
	},
};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::error::IOErrorToError;

/// Create a Command with basic ffmpeg options
#[inline]
#[must_use]
pub fn base_ffmpeg(overwrite: bool) -> Command {
	let mut cmd = super::multiplatform::spawn_command(&"ffmpeg");

	if overwrite {
		cmd.arg("-y"); // always overwrite output path
	}

	// explicitly disable interactive mode
	cmd.arg("-nostdin");

	return cmd;
}

/// Create a Command with basic ffmpeg options
/// Calls [`base_ffmpeg`] and adds argument `-hide_banner`
#[inline]
#[must_use]
pub fn base_ffmpeg_hidebanner(overwrite: bool) -> Command {
	let mut cmd = base_ffmpeg(overwrite);

	cmd.arg("-hide_banner");

	return cmd;
}

/// Regex to parse the version from a "ffmpeg -version" output
/// cap1: version (semver or git hash)
static FFMPEG_VERSION_REGEX: Lazy<Regex> = Lazy::new(|| {
	return Regex::new(r"(?mi)^ffmpeg version ([a-z0-9.-]+) Copyright").unwrap();
});

/// Helper to consistently create a error
pub(crate) fn unsuccessfull_command_exit(status: std::process::ExitStatus, output: &str) -> crate::Error {
	debug!("FFMPEG STDERR Output: {}", output);

	let last_lines = output.lines().rev().take(5).collect::<String>();

	let code_or_signal = if let Some(code) = status.code() {
		format!("Exit Code: {code}")
	} else if let Some(signal) = status.signal() {
		format!("Exit Signal: {}", signal)
	} else {
		unreachable!("There should either be a code or signal!");
	};

	return crate::Error::command_unsuccessful(format!(
		"FFMPEG did not successfully exit! {code_or_signal}\nLast Lines:\n{last_lines}"
	));
}

/// Get Version of `ffmpeg`
#[inline]
pub fn ffmpeg_version() -> Result<String, crate::Error> {
	let mut cmd = base_ffmpeg(false);
	cmd.arg("-version");

	let command_output: Output = cmd
		.stderr(Stdio::null())
		.stdout(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()
		.attach_location_err("ffmpeg spawn")?
		.wait_with_output()
		.attach_location_err("ffmpeg wait_with_output")?;

	let as_string = String::from_utf8(command_output.stdout)?;

	if !command_output.status.success() {
		return Err(unsuccessfull_command_exit(command_output.status, &as_string));
	}

	return ffmpeg_parse_version(&as_string);
}

/// Internal Function to parse the input to a ffmpeg version with regex
#[inline]
fn ffmpeg_parse_version(input: &str) -> Result<String, crate::Error> {
	return Ok(FFMPEG_VERSION_REGEX
		.captures_iter(input)
		.next()
		.ok_or_else(|| return crate::Error::no_captures("FFMPEG Version could not be determined"))?[1]
		.to_owned());
}

/// Probe a input file for information (without having to use ffprobe)
#[inline]
pub fn ffmpeg_probe<P>(input: P) -> Result<String, crate::Error>
where
	P: AsRef<OsStr>,
{
	let input = input.as_ref();
	let mut cmd = base_ffmpeg_hidebanner(false);
	cmd.arg("-i");
	cmd.arg(input);

	let command_output: Output = cmd
		.stderr(Stdio::piped()) // using stderr, because ffmpeg outputs this data on stderr
		.stdout(Stdio::null())
		.stdin(Stdio::null())
		.spawn()
		.attach_location_err("ffmpeg spawn")?
		.wait_with_output()
		.attach_location_err("ffmpeg wait_with_output")?;

	let mut was_success = true;

	let as_string = String::from_utf8_lossy(&command_output.stderr);

	// check if the output contains this one string, because ffmpeg does not offer a "probe" mode without using "ffprobe"
	// and will always exit with "1" and this message if that happens
	if command_output.status.code() == Some(1) {
		was_success = as_string.contains("At least one output file must be specified");
	}

	if !command_output.status.success() && !was_success {
		return Err(unsuccessfull_command_exit(command_output.status, &as_string));
	}

	return Ok(as_string.into());
}

/// Regex to parse the format from "input #0" from ffmpeg output
static FFMPEG_PARSE_FORMAT: Lazy<Regex> = Lazy::new(|| {
	return Regex::new(r"(?mi)^input #0, ([\w,]+?), from '").unwrap();
});

/// Parse the output from [ffmpeg_probe] to only get the format for Input 0
/// Returns a Vector of all the formats the input could be in
#[inline]
pub fn parse_format(input: &str) -> Result<Vec<&str>, crate::Error> {
	let formats = FFMPEG_PARSE_FORMAT
		.captures_iter(input)
		.next()
		.ok_or_else(|| return crate::Error::no_captures("FFMPEG Format could not be determined (1)"))?
		.get(1)
		.ok_or_else(|| return crate::Error::no_captures("FFMPEG Format could not be determined (2)"))?;

	let formats_vec: Vec<&str> = formats.as_str().split(',').collect();

	return Ok(formats_vec);
}

#[cfg(test)]
mod test {
	use super::ffmpeg_version;

	#[test]
	pub fn test_ffmpeg_parse_version_invalid_input() {
		assert_eq!(
			super::ffmpeg_parse_version("hello"),
			Err(crate::Error::no_captures("FFMPEG Version could not be determined"))
		);
	}

	#[test]
	pub fn test_ffmpeg_parse_version_valid_static_input() {
		let ffmpeg_output = "ffmpeg version n4.4.1 Copyright (c) 2000-2021 the FFmpeg developers
built with gcc 11.1.0 (GCC)
configuration: --prefix=/usr --disable-debug --disable-static --disable-stripping --enable-amf --enable-avisynth --enable-cuda-llvm --enable-lto --enable-fontconfig --enable-gmp --enable-gnutls --enable-gpl --enable-ladspa --enable-libaom --enable-libass --enable-libbluray --enable-libdav1d --enable-libdrm --enable-libfreetype --enable-libfribidi --enable-libgsm --enable-libiec61883 --enable-libjack --enable-libmfx --enable-libmodplug --enable-libmp3lame --enable-libopencore_amrnb --enable-libopencore_amrwb --enable-libopenjpeg --enable-libopus --enable-libpulse --enable-librav1e --enable-librsvg --enable-libsoxr --enable-libspeex --enable-libsrt --enable-libssh --enable-libsvtav1 --enable-libtheora --enable-libv4l2 --enable-libvidstab --enable-libvmaf --enable-libvorbis --enable-libvpx --enable-libwebp --enable-libx264 --enable-libx265 --enable-libxcb --enable-libxml2 --enable-libxvid --enable-libzimg --enable-nvdec --enable-nvenc --enable-shared --enable-version3
libavutil      56. 70.100 / 56. 70.100
libavcodec     58.134.100 / 58.134.100
libavformat    58. 76.100 / 58. 76.100
libavdevice    58. 13.100 / 58. 13.100
libavfilter     7.110.100 /  7.110.100
libswscale      5.  9.100 /  5.  9.100
libswresample   3.  9.100 /  3.  9.100
libpostproc    55.  9.100 / 55.  9.100
";

		assert_eq!(super::ffmpeg_parse_version(ffmpeg_output), Ok("n4.4.1".to_owned()));
	}

	#[test]
	pub fn test_parse_format_invalid_input() {
		assert_eq!(
			super::parse_format("hello"),
			Err(crate::Error::no_captures("FFMPEG Format could not be determined (1)"))
		);
	}

	#[test]
	pub fn test_parse_format_valid_static_input() {
		let ffmpeg_output_mkv = r#"[matroska,webm @ 0xaabbccddff11] Could not find codec parameters for stream 2 (Attachment: none): unknown codec
Consider increasing the value for the 'analyzeduration' (0) and 'probesize' (5000000) options
Input #0, matroska,webm, from 'test.mkv':
	Metadata:
	title           : Some Title
	ARTIST          : Test
	DATE            : 20210205
	DESCRIPTION     : Test Description
	ENCODER         : Lavf59.27.100
	Duration: 00:03:00.00, start: -0.007000, bitrate: 1371 kb/s
	Stream #0:0(eng): Video: vp9 (Profile 0), yuv420p(tv, bt709), 1920x1080, SAR 1:1 DAR 16:9, 23.98 fps, 23.98 tbr, 1k tbn (default)
	Metadata:
		DURATION        : 00:03:00.00
	Stream #0:1(eng): Audio: opus, 48000 Hz, stereo, fltp (default)
	Metadata:
		DURATION        : 00:03:00.00
"#;

		assert_eq!(super::parse_format(ffmpeg_output_mkv), Ok(vec!["matroska", "webm"]));

		let ffmpeg_output_mp4 = r#"Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'testep1.mp4':
Metadata:
	title           : Some Title
	artist          : Test
	date            : 20210205
	encoder         : Lavf59.27.100
	description     : Test Description
Duration: 00:03:00.00, start: 0.000000, bitrate: 4041 kb/s
Stream #0:0[0x1](eng): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 3955 kb/s, 23.98 fps, 23.98 tbr, 24k tbn (default)
	Metadata:
	handler_name    : VideoHandler
	vendor_id       : [0][0][0][0]
	encoder         : Lavc59.37.100 libx264
Stream #0:1[0x2](eng): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 73 kb/s (default)
	Metadata:
	handler_name    : SoundHandler
	vendor_id       : [0][0][0][0]	  
"#;

		assert_eq!(
			super::parse_format(ffmpeg_output_mp4),
			Ok(vec!["mov", "mp4", "m4a", "3gp", "3g2", "mj2"])
		);

		let ffmpeg_output_mp3 = r#"Input #0, mp3, from 'testep1.mp3':
Metadata:
	title           : Some Title
	artist          : Test
	date            : 20210205
	DESCRIPTION     : Test Description
	encoder         : Lavf59.27.100
Duration: 00:00:01.03, start: 0.023021, bitrate: 147 kb/s
Stream #0:0: Audio: mp3, 48000 Hz, stereo, fltp, 128 kb/s
	Metadata:
	encoder         : Lavc59.37
"#;

		assert_eq!(super::parse_format(ffmpeg_output_mp3), Ok(vec!["mp3"]));
	}

	#[test]
	#[ignore = "CI Install not present currently"]
	pub fn test_ffmpeg_spawn() {
		assert!(ffmpeg_version().is_ok());
	}
}
