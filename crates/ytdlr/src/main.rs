#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use flexi_logger::LogSpecification;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use libytdlr::*;
use std::{
	fs::File,
	io::{
		BufReader,
		Error as ioError,
	},
	path::PathBuf,
};

mod clap_conf;
use clap_conf::*;

mod commands;
mod logger;
mod state;
mod utils;

/// Main
fn main() -> Result<(), ioError> {
	let logger_handle = logger::setup_logger()?;

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

	log::info!("CLI Verbosity is {}", cli_matches.verbosity);

	// dont do anything if "-v" is not specified (use env / default instead)
	if cli_matches.verbosity > 0 {
		// apply cli "verbosity" argument to the log level
		logger_handle.set_new_spec(
			match cli_matches.verbosity {
				0 => unreachable!("Unreachable because it should be tested before that it is higher than 0"),
				1 => LogSpecification::parse("info"),
				2 => LogSpecification::parse("debug"),
				3 => LogSpecification::parse("trace"),
				_ => {
					return Err(ioError::new(
						std::io::ErrorKind::Other,
						"Expected verbosity integer range between 0 and 3 (inclusive)",
					))
				},
			}
			.expect("Expected LogSpecification to parse correctly"),
		);
	}

	match &cli_matches.subcommands {
		SubCommands::Download(v) => commands::download::command_download(&cli_matches, v),
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
	}?;

	return Ok(());
}

/// Handler function for the "archive import" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_import(main_args: &CliDerive, sub_args: &ArchiveImport) -> Result<(), ioError> {
	use libytdlr::main::archive::import::*;
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
			.expect("Expected ProgressStyle template to be valid")
			.progress_chars("#>-");
	}

	let bar: ProgressBar = ProgressBar::hidden().with_style(IMPORT_STYLE.clone());
	crate::utils::set_progressbar(&bar, main_args);

	let (_new_archive, mut connection) = utils::handle_connect(archive_path, &bar, main_args)?;

	let mut reader = BufReader::new(File::open(input_path)?);

	let pgcb_import = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {v} elements")),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Import"),
				ImportProgress::SizeHint(v) => println!("Import SizeHint: {v}"),
				ImportProgress::Increase(c, i) => println!("Import Increase: {c}, Current Index: {i}"),
				ImportProgress::Finished(v) => println!("Import Finished, Successfull Imports: {v}"),
				_ => (),
			}
		}
	};

	import_any_archive(&mut reader, &mut connection, pgcb_import)?;

	return Ok(());
}

/// Handler function for the "archive migrate" subcommand
/// This function is mainly to keep the code structured and sorted
// #[inline]
// fn command_migrate(main_args: &CliDerive, sub_args: &ArchiveMigrate) -> Result<(), ioError> {}

/// Handler function for the "rethumbnail" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_rethumbnail(_main_args: &CliDerive, sub_args: &CommandReThumbnail) -> Result<(), ioError> {
	use libytdlr::main::rethumbnail::*;
	utils::require_ffmpeg_installed()?;

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
