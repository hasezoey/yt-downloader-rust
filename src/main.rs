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
use std::io::Error as ioError;

mod lib;

use lib::*;

/// Main
fn main() -> Result<(), ioError> {
	builder().target(Target::Stderr).init();

	let yml = load_yaml!("./cli.yml");
	let cli_matches = App::from_yaml(yml).get_matches();

	if let Some(matches) = cli_matches.subcommand_matches("import") {
		let archive = import_archive::import_archive(&matches)?;

		setup_archive::finish_archive(&archive)?;

		return Ok(());
	}

	// mutable because it is needed for the archive
	let mut args = setup_arguments::setup_args(&cli_matches)?;

	spawn_main::spawn_ytdl(&mut args).unwrap_or_else(|err| {
		println!("An Error Occured in spawn_ytdl (still saving archive):\n\t{}", err);
	});

	if let Some(archive) = &args.archive {
		setup_archive::finish_archive(&archive)?;
	} else {
		info!("No Archive, not writing");
	}

	return Ok(());
}
