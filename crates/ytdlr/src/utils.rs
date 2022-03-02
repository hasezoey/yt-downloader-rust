//! Utils for the `ytdlr` binary

use crate::clap_conf::*;
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
};
use libytdlr::spawn::{
	ffmpeg::ffmpeg_version,
	ytdl::ytdl_version,
};
use std::io::Error as ioError;

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
