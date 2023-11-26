#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use flexi_logger::LogSpecification;
use libytdlr::{
	invoke_vscode_debugger,
	Error,
};
use once_cell::sync::Lazy;
use std::sync::RwLock;

mod clap_conf;
use clap_conf::{
	ArchiveDerive,
	ArchiveSubCommands,
	CliDerive,
	SubCommands,
};

mod commands;
mod logger;
mod state;
mod utils;

/// Simple struct to keep all data for termination requests (ctrlc handler)
struct TerminateData {
	/// Stores whether the handler is enabled or disabled
	/// "disabled" means no termination setting
	enabled:             bool,
	/// Stores whether termination has been requested
	terminate_requested: bool,
}

impl Default for TerminateData {
	fn default() -> Self {
		return TerminateData {
			enabled:             true,
			terminate_requested: false,
		};
	}
}

impl TerminateData {
	/// Check if termination has been requested
	pub fn termination_requested(&self) -> bool {
		return self.terminate_requested;
	}

	/// Set that termination has been requested
	pub fn set_terminate(&mut self) {
		self.terminate_requested = true;
	}

	/// Set handler to be disabled until re-enabled
	pub fn disable(&mut self) {
		self.enabled = false;
	}

	/// Re-enable handler
	pub fn enable(&mut self) {
		self.enabled = true;
	}

	/// Get whether the handler is enabled or not
	pub fn is_enabled(&self) -> bool {
		return self.enabled;
	}
}

/// Default Termination request message
const TERMINATE_MSG: &str = "Termination requested, press again to terminate immediately";

/// Global instance of [TerminateData] for termination handling
static TERMINATE: Lazy<RwLock<TerminateData>> = Lazy::new(|| {
	return RwLock::new(TerminateData::default());
});

/// Main
fn main() {
	let res = actual_main();

	if let Err(err) = res {
		eprintln!("A Error occured:\n{err}");
		let backtrace = err.get_backtrace();
		match backtrace.status() {
			std::backtrace::BacktraceStatus::Captured => eprintln!("Backtrace:\n{}", backtrace),
			std::backtrace::BacktraceStatus::Disabled => {
				eprintln!("Backtrace is disabled, enable with RUST_BACKTRACE=true");
			},
			_ => eprintln!("Backtrace is unsupported"),
		}
		std::process::exit(1);
	}
}

/// Actually the main function, to be wrapped in a custom error handler
fn actual_main() -> Result<(), crate::Error> {
	let logger_handle = logger::setup_logger();

	let cli_matches = CliDerive::custom_parse()?;

	if cli_matches.debugger_enabled() {
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

		if terminate_write.termination_requested() {
			info!("Immediate Termination requested");
			std::process::exit(-1);
		}
		println!("{}", TERMINATE_MSG);
		terminate_write.set_terminate();
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

	return match &cli_matches.subcommands {
		SubCommands::Download(v) => commands::download::command_download(&cli_matches, v),
		SubCommands::Archive(v) => sub_archive(&cli_matches, v),
		SubCommands::ReThumbnail(v) => commands::rethumbnail::command_rethumbnail(&cli_matches, v),
		SubCommands::Completions(v) => commands::completions::command_completions(&cli_matches, v),
	};
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
