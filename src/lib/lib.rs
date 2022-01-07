#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

pub mod archive_schema;
pub mod ask_edit;
pub mod file_cleanup;
pub mod import_archive;
pub mod move_finished;
pub mod paths;
pub mod setup_archive;
pub mod setup_arguments;
pub mod spawn_main;
pub mod spawn_multi_platform;
pub mod utils;
