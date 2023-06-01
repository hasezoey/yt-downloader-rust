#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use flexi_logger::LogSpecification;
use libytdlr::*;
use once_cell::sync::Lazy;
use std::sync::RwLock;

mod clap_conf;
use clap_conf::*;

mod commands;
mod logger;
mod state;
mod utils;

/// Simple struct to keep all data for termination requests (ctrlc handler)
struct TerminateData {
	/// Stores the last time a terminate was requested, if ever
	terminate: Option<std::time::Instant>,
	/// Stores the message to display when pressing CTRLC
	msg:       String,
	/// Stores wheter the handler is enabled or disabled
	/// "disabled" means no termination setting
	enabled:   bool,
}

impl Default for TerminateData {
	fn default() -> Self {
		return TerminateData {
			terminate: None,
			msg:       String::from(DEFAULT_TERMINATE_MSG),
			enabled:   true,
		};
	}
}

impl TerminateData {
	/// Check if a Termination is requested and still valid
	pub fn should_terminate(&self) -> bool {
		let inst = match self.terminate {
			Some(v) => v,
			None => return false,
		};

		return inst.elapsed().as_secs() <= 3;
	}

	/// Set the time when the terminate was requested
	pub fn set_terminate_time(&mut self) {
		self.terminate = Some(std::time::Instant::now());
	}

	/// Get the termination message
	pub fn get_msg(&self) -> &String {
		return &self.msg;
	}

	/// Set the termination message
	pub fn set_msg(&mut self, msg: String) {
		self.msg = msg;
	}

	/// Set handler to be disabled until re-enabled
	pub fn disable(&mut self) {
		self.enabled = false;
	}

	/// Re-enable handler
	pub fn enable(&mut self) {
		self.enabled = true;
	}

	/// Get wheter the handler is enabled or not
	pub fn is_enabled(&self) -> bool {
		return self.enabled;
	}
}

/// Default Termination request message
const DEFAULT_TERMINATE_MSG: &str = "Press Again to Terminate within the next 3 seconds";

/// Global instance of [TerminateData] for termination handling
static TERMINATE: Lazy<RwLock<TerminateData>> = Lazy::new(|| {
	return RwLock::new(TerminateData::default());
});

/// Main
fn main() -> Result<(), crate::Error> {
	let logger_handle = logger::setup_logger()?;

	let cli_matches = CliDerive::custom_parse()?;

	if cli_matches.debug_enabled() {
		warn!("Requesting Debugger");

		#[cfg(debug_assertions)]
		{
			invoke_vscode_debugger();
		}
	}

	// basic crtlc handler, may not be the best method
	ctrlc::set_handler(move || {
		// dont run handler if handler is meant to be disabled
		if !TERMINATE
			.read()
			.expect("Should be able to acquire read lock")
			.is_enabled()
		{
			return;
		}

		let mut tries = 5;

		let mut terminate_write;

		loop {
			if tries == 0 {
				println!("failed to acquire write-lock, immediately exiting");
				std::process::exit(-1);
			}
			tries -= 1;
			if let Ok(v) = TERMINATE.try_write() {
				terminate_write = v;
				break;
			}

			warn!(
				"crtlc: Acquiring write-lock takes longer than expected! Remaining tries: {}",
				tries
			);
			// only wait as long as there are tries
			if tries > 0 {
				std::thread::sleep(std::time::Duration::from_millis(500)); // sleep 500ms to not immediately try again
			}
		}

		if terminate_write.should_terminate() {
			std::process::exit(-1);
		}
		println!("{}", terminate_write.get_msg());
		terminate_write.set_terminate_time();
	})
	.map_err(|err| return crate::Error::other(format!("{err}")))?;

	log::info!("CLI Verbosity is {}", cli_matches.verbosity);

	colored::control::set_override(cli_matches.enable_colors());

	// dont do anything if "-v" is not specified (use env / default instead)
	if cli_matches.verbosity > 0 {
		// apply cli "verbosity" argument to the log level
		logger_handle.set_new_spec(
			match cli_matches.verbosity {
				0 => unreachable!("Unreachable because it should be tested before that it is higher than 0"),
				1 => LogSpecification::parse("info"),
				2 => LogSpecification::parse("debug"),
				3 => LogSpecification::parse("trace"),
				_ => {
					return Err(crate::Error::other(
						"Expected verbosity integer range between 0 and 3 (inclusive)",
					))
				},
			}
			.expect("Expected LogSpecification to parse correctly"),
		);
	}

	let res = match &cli_matches.subcommands {
		SubCommands::Download(v) => commands::download::command_download(&cli_matches, v),
		SubCommands::Archive(v) => sub_archive(&cli_matches, v),
		SubCommands::ReThumbnail(v) => commands::rethumbnail::command_rethumbnail(&cli_matches, v),
		SubCommands::Completions(v) => commands::completions::command_completions(&cli_matches, v),
	};

	if let Err(err) = res {
		eprintln!("A Error occured:\n{err}");
		std::process::exit(1);
	}

	return Ok(());
}

/// Handler function for the "archive" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn sub_archive(main_args: &CliDerive, sub_args: &ArchiveDerive) -> Result<(), crate::Error> {
	match &sub_args.subcommands {
		ArchiveSubCommands::Import(v) => commands::import::command_import(main_args, v),
		ArchiveSubCommands::Search(v) => commands::search::command_search(main_args, v),
	}?;

	return Ok(());
}
