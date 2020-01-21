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

use env_logger::{
	builder,
	Target,
};
use std::io::Error as ioError;

mod lib;

use lib::*;
use setup_archive::finish_archive;
use setup_arguments::setup_args;
use spawn_main::spawn_ytdl;

/// Main
fn main() -> Result<(), ioError> {
	builder().target(Target::Stderr).init();

	// mutable because it is needed for the archive
	let mut args = setup_args()?;

	spawn_ytdl(&mut args).unwrap_or_else(|err| {
		println!("An Error Occured in spawn_ytdl (still saving archive):\n\t{}", err);
	});

	finish_archive(&args)?;

	return Ok(());
}
