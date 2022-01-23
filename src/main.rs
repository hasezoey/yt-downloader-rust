#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use clap::{
	AppSettings,
	Parser,
};
use env_logger::{
	builder,
	Target,
};
use std::io::Error as ioError;
use std::path::PathBuf;

use libytdlr::*;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(bin_name("ytdlr"))]
#[clap(global_setting(AppSettings::AllArgsOverrideSelf))] // specifying a argument multiple times overwrites the earlier ones
#[clap(global_setting(AppSettings::DisableHelpSubcommand))] // Disable subcommand "help", only "-h --help" should be used
#[clap(global_setting(AppSettings::SubcommandsNegateReqs))]
pub struct CliDerive {
	#[clap(short, long, parse(from_os_str), env = "YTDL_OUT")]
	pub output:               Option<PathBuf>,
	#[clap(long, parse(from_os_str), env = "YTDL_TMP")]
	pub tmp:                  Option<PathBuf>,
	#[clap(short = 'a')]
	pub audio_only:           bool,
	#[clap(short, long)]
	pub debug:                bool,
	#[clap(long)]
	pub debugger:             bool,
	#[clap(short = 'c')]
	pub disable_cleanup:      bool,
	#[clap(short = 't')]
	pub disable_re_thumbnail: bool,
	#[clap(long, parse(from_os_str), env = "YTDL_ARCHIVE")]
	pub archive:              Option<PathBuf>,
	#[clap(short = 'e')]
	pub disable_askedit:      bool,
	#[clap(long, env = "YTDL_EDITOR")]
	pub editor:               Option<String>,

	// #[clap(subcommand)]
	// pub subcommands: SubCommands,

	// #[clap(last = true)]
	pub url: String,
}

impl CliDerive {
	pub fn custom_parse() -> Self {
		let parsed = Self::parse();

		if parsed.editor.is_none() {
			panic!("Editor needs to be set!");
		}

		return parsed;
	}
}

// #[derive(Debug, Subcommand)]
// pub enum SubCommands {
// 	Import(CommandImport),
// }

// impl SubCommands {
// 	pub fn get_import(&self) -> Option<&CommandImport> {
// 		return match &self {
// 			Self::Import(v) => Some(&v),
// 			_ => None,
// 		};
// 	}
// }

// #[derive(Debug, Parser)]
// pub struct CommandImport {
// 	#[clap(parse(from_os_str))]
// 	pub input: PathBuf,
// }

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
