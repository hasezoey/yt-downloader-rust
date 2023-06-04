//! Library of "YTDL-Rust", contains all the logic needed for the binary

#![allow(clippy::needless_return)]
#![allow(special_module_name)] // because of module "main", dont have a better name for that
#![warn(clippy::implicit_return)]
// #![deny(missing_docs)]

#[macro_use]
extern crate log;

pub mod data;
pub mod error;
pub mod main;
pub mod spawn;
pub mod traits;
pub mod utils;
pub use error::Error;

/// Debug function to start vscode-lldb debugger from external console
/// Only compiled when the target is "debug"
#[cfg(debug_assertions)]
pub fn invoke_vscode_debugger() {
	println!("Requesting Debugger");
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

	println!("Press ENTER to continue");
	let _ = std::io::stdin().read_line(&mut String::new()); // wait until attached, then press ENTER to continue
}

pub use chrono;
pub use diesel;
