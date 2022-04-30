//! Utils for the `ytdlr` binary

use crate::clap_conf::*;
use diesel::SqliteConnection;
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
};
use libytdlr::{
	main::archive::import::ImportProgress,
	spawn::{
		ffmpeg::ffmpeg_version,
		ytdl::ytdl_version,
	},
};
use std::{
	borrow::Cow,
	io::Error as ioError,
	path::Path,
};

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

/// Generic handler function for using [`libytdlr::main::sql_utils::migrate_and_connect`] with a [`ProgressBar`]
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
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Migrating {} elements", v)),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Migration"),
				ImportProgress::SizeHint(v) => println!("Migration SizeHint: {}", v),
				ImportProgress::Increase(c, i) => println!("Migration Increase: {}, Current Index: {}", c, i),
				ImportProgress::Finished(v) => println!("Migration Finished, Successfull Migrations: {}", v),
				_ => (),
			}
		}
	};

	let res = libytdlr::main::sql_utils::migrate_and_connect(archive_path, pgcb_migrate)?;

	if res.0 != archive_path {
		bar.finish_with_message(format!(
			"Migration from JSON to SQLite archive done, Archive path has changed to \"{}\"",
			res.0.to_string_lossy()
		));
	} else {
		bar.finish_and_clear();
	}

	return Ok(res);
}
