#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
extern crate clap;
extern crate colored;
extern crate indicatif;
extern crate regex;
extern crate serde;

use std::io::Error as ioError;

mod lib;

use lib::*;
use setup_arguments::setup_args;
use spawn_main::spawn_ytdl;

/// Main
fn main() -> Result<(), ioError> {
	let args = setup_args()?;

	spawn_ytdl(&args)?;

	return Ok(());
}
