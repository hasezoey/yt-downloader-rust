#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate clap;
extern crate colored;
extern crate env_logger;
extern crate indicatif;
extern crate regex;
extern crate semver;
extern crate serde;
extern crate serde_json;

use clap::load_yaml;
use clap::App;
use env_logger::{
	builder,
	Target,
};
use std::io::{
	Error as ioError,
	Write,
};

mod lib;

use lib::*;

use utils::Arguments;

fn trim_newline(s: &mut String) {
	if s.ends_with('\n') {
		s.pop();
		if s.ends_with('\r') {
			s.pop();
		}
	}
}

/// Main
fn main() -> Result<(), ioError> {
	builder().target(Target::Stderr).init();

	let yml = load_yaml!("./cli.yml");
	let cli_matches = App::from_yaml(yml).get_matches();

	// handle importing native youtube-dl archives
	if let Some(matches) = cli_matches.subcommand_matches("import") {
		let archive = import_archive::import_archive(&matches)?;

		setup_archive::finish_archive(&archive)?;

		return Ok(());
	}

	// handle command without subcommands (actually downloading)

	// mutable because it is needed for the archive
	let mut args = setup_arguments::setup_args(&cli_matches)?;
	let mut errcode = false;

	spawn_main::spawn_ytdl(&mut args).unwrap_or_else(|err| {
		println!("An Error Occured in spawn_ytdl (still saving archive):\n\t{}", err);
		errcode = true;
	});

	if !errcode && args.askedit {
		if let Some(archive) = &args.archive {
			edits(&mut args).unwrap_or_else(|err| {
				println!("An Error Occured in edits:\n\t{}", err);
			});
		} else {
			info!("No Archive, not asking for edits");
		}
	}

	if let Some(archive) = &args.archive {
		setup_archive::finish_archive(&archive)?;
	} else {
		info!("No Archive, not writing");
	}

	// if an error happened, exit with an non-zero error code
	if errcode {
		warn!("Existing with non-zero code, because of an previous Error");
		std::process::exit(1);
	}
	return Ok(());
}

/// Ask for edits on donwloaded files
fn edits(args: &mut Arguments) -> Result<(), ioError> {
	debug!("Asking for Edit");
	if args.editor.len() <= 0 {
		println!("Please enter an command to be used as editor, or leave it empty to skip it");
		print!("$ ");
		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		trim_newline(&mut input); // trim the newline at the end
		args.editor = input.trim().to_owned();
		debug!("Editor entered: {}", args.editor);

		if args.editor.len() <= 0 {
			// if it is still empty, just dont ask for edits
			info!("Editor is empty, not asking for edits");
			return Ok(());
		}
	}
	// TODO: Ask for Edit

	return Ok(());
}
