#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate clap;
extern crate colored;
extern crate env_logger;
extern crate indicatif;
extern crate regex;
extern crate semver;
extern crate serde;

use env_logger::{
	builder,
	Target,
};
use std::io::Error as ioError;

mod lib;

use lib::*;
use setup_arguments::setup_args;
use spawn_main::spawn_ytdl;

/// Main
fn main() -> Result<(), ioError> {
	builder().target(Target::Stderr).init();

	let args = setup_args()?;

	spawn_ytdl(&args)?;

	return Ok(());
}
