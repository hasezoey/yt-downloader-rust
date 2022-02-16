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
};

use libytdlr::*;

mod clap_conf;
use clap_conf::*;

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

	// Note: Subcommands are disabled until re-writing with subcommands
	// handle importing native youtube-dl archives
	// if let Some(sub_matches) = cli_matches.subcommands.get_import() {
	// 	debug!("Subcommand \"import\" is given");
	// 	let archive = import_archive::import_archive(import_archive::CommandImport {
	// 		input:   sub_matches.input.clone(),
	// 		archive: cli_matches
	// 			.archive
	// 			.expect("Archive path needs to be defined for Subcommand \"import\""),
	// 	})?;

	// 	setup_archive::write_archive(&archive)?;

	// 	return Ok(());
	// }

	// DEBUG
	// println!("command: {:#?}", cli_matches);
	// std::process::exit(0);

	// handle command without subcommands (actually downloading)

	// mutable because it is needed for the archive
	let mut args = setup_arguments::setup_args(setup_arguments::SetupArgs {
		out:                  cli_matches.output,
		tmp:                  cli_matches.tmp,
		url:                  cli_matches.url,
		archive:              cli_matches.archive,
		audio_only:           cli_matches.audio_only,
		debug:                cli_matches.debug,
		disable_cleanup:      cli_matches.disable_cleanup,
		disable_re_thumbnail: cli_matches.disable_re_thumbnail,
		askedit:              !cli_matches.disable_askedit, // invert, because of old implementation
		editor:               cli_matches.editor.expect("Expected editor to be set!"),
	})?;
	let mut errcode = false;

	spawn_main::spawn_ytdl(&mut args).unwrap_or_else(|err| {
		println!(
			"An Error Occured in spawn_ytdl (still saving archive to tmp):\n\t{}",
			err
		);
		errcode = true;
	});

	if !errcode && args.askedit {
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
		move_finished::move_finished_files(&args)?;
	}

	if let Some(archive) = &mut args.archive {
		if errcode {
			debug!("An Error occured, writing archive to TMP location");
			archive.path = args.tmp.join("ytdl_archive_ERR.json");
		}

		setup_archive::write_archive(archive)?;
	} else {
		info!("No Archive, not writing");
	}

	if !errcode && !args.disable_cleanup {
		file_cleanup::file_cleanup(&args)?;
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
fn command_import(main_args: CliDerive, sub_args: CommandImport) -> Result<(), ioError> {
	use indicatif::{
		ProgressBar,
		ProgressStyle,
	};
	println!("Importing Archive from \"{}\"", sub_args.file.to_string_lossy());

	let input_path = sub_args.file;

	if main_args.archive.is_none() {
		return Err(ioError::new(
			std::io::ErrorKind::Other,
			"Archive is required for Import!",
		));
	}

	let archive_path = main_args
		.archive
		.expect("Expected archive check to have already returned");

	lazy_static::lazy_static! {
		static ref IMPORT_STYLE: ProgressStyle = ProgressStyle::default_bar()
			.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
			.progress_chars("#>-");
	}

	let bar: ProgressBar = ProgressBar::new(0).with_style(IMPORT_STYLE.clone());

	let mut archive = if let Some(archive) = libytdlr::setup_archive::setup_archive(archive_path) {
		archive
	} else {
		return Err(ioError::new(std::io::ErrorKind::Other, "Reading Archive failed!"));
	};

	let mut reader = BufReader::new(File::open(input_path)?);

	let pgcb = |imp| match imp {
		ImportProgress::Starting => todo!(),
		ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
		ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
		ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {} elements", v)),
		_ => (),
	};

	libytdlr::import_any_archive(&mut reader, &mut archive, pgcb)?;

	return Ok(());
}
