//! Module that contains all logic for spawning the "ffmpeg" command
use std::process::Command;
use std::process::{
	Output,
	Stdio,
};

use regex::Regex;

#[inline]
pub fn base_ffmpeg(overwrite: bool) -> Command {
	let mut cmd = super::multiplatform::spawn_command("ffmpeg");

	if overwrite {
		cmd.arg("-y"); // always overwrite output path
	}

	// explicitly disable interactive mode
	cmd.arg("-nostdin");

	return cmd;
}

lazy_static! {
	static ref FFMPEG_VERSION_REGEX: Regex = Regex::new(r"(?mi)^ffmpeg version ([a-z0-9.-]+) Copyright").unwrap();
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
		.spawn()?
		.wait_with_output()?;

	if !command_output.status.success() {
		return Err(crate::Error::CommandNotSuccesfull(
			"FFMPEG did not successfully exit!".to_string(),
		));
	}

	let as_string = String::from_utf8(command_output.stdout)?;

	return ffmpeg_parse_version(&as_string);
}

/// Internal Function to parse the input to a ffmpeg version with regex
#[inline]
fn ffmpeg_parse_version(input: &str) -> Result<String, crate::Error> {
	return Ok(FFMPEG_VERSION_REGEX
		.captures_iter(input)
		.next()
		.ok_or_else(|| return crate::Error::NoCapturesFound("FFMPEG Version could not be determined".to_owned()))?[1]
		.to_owned());
}

#[cfg(test)]
mod test {
	use super::ffmpeg_version;

	#[test]
	pub fn test_ffmpeg_parse_version_invalid_input() {
		assert_eq!(
			super::ffmpeg_parse_version("hello"),
			Err(crate::Error::NoCapturesFound(
				"FFMPEG Version could not be determined".to_owned()
			))
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
	#[ignore = "CI Install not present currently"]
	pub fn test_ffmpeg_spawn() {
		assert!(ffmpeg_version().is_ok());
	}
}
