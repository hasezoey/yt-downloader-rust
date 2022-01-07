#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use clap::load_yaml;
use clap::App;
use env_logger::{
	builder,
	Target,
};
use std::io::Error as ioError;

use libytdlr::*;

/// Main
fn main() -> Result<(), ioError> {
	// logging to stdout because nothing else is on there and to not interfere with the progress bars
	builder().target(Target::Stdout).init();

	let yml = load_yaml!("./cli.yml");
	let cli_matches = App::from_yaml(yml).get_matches();

	if cli_matches.is_present("debugger") {
		warn!("Requesting Debugger");
		// Request VSCode to open a debugger for the current PID
		let url = format!(
			"vscode://vadimcn.vscode-lldb/launch/config?{{'request':'attach','pid':{}}}",
			std::process::id()
		);
		std::process::Command::new("code")
			.arg("--open-url")
			.arg(url)
			.output()
			.unwrap();
		std::thread::sleep(std::time::Duration::from_millis(1000)); // Wait for debugger to attach
	}

	// handle importing native youtube-dl archives
	if let Some(sub_matches) = cli_matches.subcommand_matches("import") {
		debug!("Subcommand \"import\" is given");
		let archive = import_archive::import_archive(sub_matches, &cli_matches)?;

		setup_archive::write_archive(&archive)?;

		return Ok(());
	}

	// handle command without subcommands (actually downloading)

	// mutable because it is needed for the archive
	let mut args = setup_arguments::setup_args(&cli_matches)?;
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
