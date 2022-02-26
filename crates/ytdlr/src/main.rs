#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use env_logger::{
	builder,
	Target,
};
use std::{
	fs::File,
	io::{
		BufReader,
		Error as ioError,
	},
	path::PathBuf,
};

use libytdlr::*;

mod clap_conf;
use clap_conf::*;
mod utils;

/// Main
fn main() -> Result<(), ioError> {
	// logging to stdout because nothing else is on there and to not interfere with the progress bars
	builder().target(Target::Stdout).init();

	let cli_matches = CliDerive::custom_parse();

	if cli_matches.debugger {
		warn!("Requesting Debugger");

		#[cfg(debug_assertions)]
		{
			invoke_vscode_debugger();
		}
		#[cfg(not(debug_assertions))]
		{
			println!("Debugger Invokation only available in Debug Target");
		}
	}

	// env_logger can currently not dynamically set the loglevel
	// println!("test veb, {}", cli_matches.verbosity);

	// match cli_matches.verbosity {
	// 	0 => log::set_max_level(log::LevelFilter::Warn),
	// 	1 => log::set_max_level(log::LevelFilter::Info),
	// 	2 => log::set_max_level(log::LevelFilter::Debug),
	// 	3 => log::set_max_level(log::LevelFilter::Trace),
	// 	_ => {
	// 		return Err(ioError::new(
	// 			std::io::ErrorKind::Other,
	// 			"Expected verbosity integer range between 0 and 3 (inclusive)",
	// 		))
	// 	},
	// }

	// log::error!("test error");
	// log::warn!("test warn");
	// log::info!("test info");
	// log::debug!("test debug");
	// log::trace!("test trace");

	// todo!();

	match &cli_matches.subcommands {
		SubCommands::Download(v) => command_download(&cli_matches, v),
		SubCommands::Archive(v) => sub_archive(&cli_matches, v),
		SubCommands::ReThumbnail(v) => command_rethumbnail(&cli_matches, v),
	}?;

	return Ok(());
}

/// Handler function for the "archive" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn sub_archive(main_args: &CliDerive, sub_args: &ArchiveDerive) -> Result<(), ioError> {
	match &sub_args.subcommands {
		ArchiveSubCommands::Import(v) => command_import(main_args, v),
		// ArchiveSubCommands::Migrate(_v) => todo!(),
	}?;

	return Ok(());
}

/// Handler function for the "download" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), ioError> {
	if sub_args.urls.is_empty() {
		return Err(ioError::new(std::io::ErrorKind::Other, "At least one URL is required"));
	}
	if sub_args.urls.len() > 1 {
		return Err(ioError::new(
			std::io::ErrorKind::Other,
			"Currently only One (1) URL is supported",
		));
	}

	let mut errcode = false;
	let mut tmp = std::env::temp_dir();

	{
		let mut args = setup_arguments::setup_args(setup_arguments::SetupArgs {
			out:                  sub_args.output_path.clone(),
			tmp:                  main_args.tmp_path.clone(),
			url:                  sub_args.urls[0].clone(),
			archive:              main_args.archive_path.clone(),
			audio_only:           sub_args.audio_only_enable,
			debug:                main_args.verbosity >= 2,
			disable_re_thumbnail: sub_args.reapply_thumbnail_disable,
			editor:               sub_args
				.audio_editor
				.as_ref()
				.expect("Expected editor to be set!")
				.to_string_lossy()
				.to_string(),
		})?;

		spawn_main::spawn_ytdl(&mut args).unwrap_or_else(|err| {
			println!(
				"An Error Occured in spawn_ytdl (still saving archive to tmp):\n\t{}",
				err
			);
			errcode = true;
		});

		if !errcode && main_args.is_interactive() {
			if args.archive.is_some() {
				ask_edit::edits(&mut args).unwrap_or_else(|err| {
					println!("An Error Occured in edits:\n\t{}", err);
					errcode = true;
				});
			} else {
				info!("No Archive, not asking for edits");
			}
		}

		if !errcode {
			move_finished::move_finished_files(&args.out, &args.tmp, args.debug)?;
		}

		if let Some(archive) = &mut args.archive {
			if errcode {
				debug!("An Error occured, writing archive to TMP location");
				archive.path = tmp.join("ytdl_archive_ERR.json");
			}

			setup_archive::write_archive(&archive)?;
		} else {
			info!("No Archive, not writing");
		}

		tmp = args.tmp;
	}

	if !errcode {
		std::fs::remove_dir_all(&tmp)?;
	}

	// if an error happened, exit with an non-zero error code
	if errcode {
		warn!("Existing with non-zero code, because of an previous Error");
		std::process::exit(1);
	}
	return Ok(());
}

/// Handler function for the "import" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_import(main_args: &CliDerive, sub_args: &ArchiveImport) -> Result<(), ioError> {
	use indicatif::{
		ProgressBar,
		ProgressStyle,
	};
	println!("Importing Archive from \"{}\"", sub_args.file_path.to_string_lossy());

	let input_path = &sub_args.file_path;

	if main_args.archive_path.is_none() {
		return Err(ioError::new(
			std::io::ErrorKind::Other,
			"Archive is required for Import!",
		));
	}

	let archive_path = main_args
		.archive_path
		.as_ref()
		.expect("Expected archive check to have already returned");

	lazy_static::lazy_static! {
		static ref IMPORT_STYLE: ProgressStyle = ProgressStyle::default_bar()
			.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
			.progress_chars("#>-");
	}

	let bar: ProgressBar = ProgressBar::hidden().with_style(IMPORT_STYLE.clone());

	let mut archive = if let Some(archive) = libytdlr::setup_archive::setup_archive(archive_path) {
		archive
	} else {
		return Err(ioError::new(std::io::ErrorKind::Other, "Reading Archive failed!"));
	};

	let mut reader = BufReader::new(File::open(input_path)?);

	crate::utils::set_progressbar(&bar, main_args);

	let pgcb = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {} elements", v)),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Import"),
				ImportProgress::SizeHint(v) => println!("Import SizeHint: {}", v),
				ImportProgress::Increase(c, i) => println!("Import Increase: {}, Current Index: {}", c, i),
				ImportProgress::Finished(v) => println!("Import Finished, Successfull Imports: {}", v),
				_ => (),
			}
		}
	};

	libytdlr::import_any_archive(&mut reader, &mut archive, pgcb)?;

	return Ok(());
}

/// Handler function for the "rethumbnail" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_rethumbnail(_main_args: &CliDerive, sub_args: &CommandReThumbnail) -> Result<(), ioError> {
	// helper aliases to make it easier to access
	let input_image_path: &PathBuf = &sub_args.input_image_path;
	let input_media_path: &PathBuf = &sub_args.input_media_path;
	let output_media_path: &PathBuf = sub_args
		.output_media_path
		.as_ref()
		.expect("Expected trait \"Check\" to be run on \"CommandReThumbnail\" before this point");

	println!(
		"Re-Applying Thumbnail image \"{}\" to media file \"{}\"",
		input_image_path.to_string_lossy(),
		input_media_path.to_string_lossy()
	);

	re_thumbnail_with_tmp(input_media_path, input_image_path, output_media_path)?;

	println!(
		"Re-Applied Thumbnail to media, as \"{}\"",
		output_media_path.to_string_lossy()
	);

	return Ok(());
}
