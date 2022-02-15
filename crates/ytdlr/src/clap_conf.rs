//! Module for Clap related structs (derived)

#![deny(missing_docs)] // comments are used for "--help" generation, so it should always be defined

use clap::{
	AppSettings,
	Parser,
	Subcommand,
};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(bin_name("ytdlr"))]
#[clap(global_setting(AppSettings::AllArgsOverrideSelf))] // specifying a argument multiple times overwrites the earlier ones
#[clap(global_setting(AppSettings::DisableHelpSubcommand))] // Disable subcommand "help", only "-h --help" should be used
#[clap(global_setting(AppSettings::SubcommandsNegateReqs))]
pub struct CliDerive {
	/// Output path for any command that outputs a file
	#[clap(short, long, parse(from_os_str), env = "YTDL_OUT")]
	pub output:               Option<PathBuf>,
	/// Temporary directory path to store intermediate files (like downloaded files before being processed)
	#[clap(long, parse(from_os_str), env = "YTDL_TMP")]
	pub tmp:                  Option<PathBuf>,
	/// Enable Audio only mode (Output will be audio-only)
	#[clap(short = 'a')]
	pub audio_only:           bool,
	/// Enable Debug logs (does not replace RUST_LOG)
	// TODO: refactor to use verbosity (count) and a quiet mode
	#[clap(short, long)]
	pub debug:                bool,
	/// Request vscode lldb debugger before continuing to execute
	#[clap(long)]
	pub debugger:             bool,
	/// Disable cleanup (TODO: clarify what this does)
	#[clap(short = 'c')]
	pub disable_cleanup:      bool,
	/// Disable re-applying the thumbnail after the editor closes
	#[clap(short = 't')]
	pub disable_re_thumbnail: bool,
	/// Archive path to use, if a archive should be used
	#[clap(long, parse(from_os_str), env = "YTDL_ARCHIVE")]
	pub archive:              Option<PathBuf>,
	/// Disable asking to edit files after download
	#[clap(short = 'e')]
	pub disable_askedit:      bool,
	/// Audio Editor to use when asking for edits
	#[clap(long, env = "YTDL_EDITOR")]
	pub editor:               Option<String>,

	// #[clap(subcommand)]
	// pub subcommands: SubCommands,
	/// The URL to download
	// #[clap(last = true)]
	pub url: String,
}

impl CliDerive {
	/// Execute clap::Parser::parse and apply custom validation and transformation logic
	pub fn custom_parse() -> Self {
		let parsed = Self::parse();

		if parsed.editor.is_none() {
			panic!("Editor needs to be set!");
		}

		return parsed;
	}
}

#[derive(Debug, Subcommand)]
pub enum SubCommands {
	/// The main purpose of the binary, download URL
	Download(CommandDownload),
	/// Import another archive (either ytdl or ytdl-r archives)
	Import(CommandImport),
	/// Re-Thumbnail specific files
	ReThumbnail(CommandReThumbnail),
	/// Generate all shell completions
	Completions(CommandCompletions),
	/// Check the Existing Archive
	CheckArchive(CommandCheckArchive),
}

// impl SubCommands {
// 	pub fn get_import(&self) -> Option<&CommandImport> {
// 		return match &self {
// 			Self::Import(v) => Some(&v),
// 			_ => None,
// 		};
// 	}
// }

#[derive(Debug, Parser)]
pub struct CommandImport {
	#[clap(parse(from_os_str))]
	pub file: PathBuf,
}

#[derive(Debug, Parser)]
pub struct CommandDownload {}

#[derive(Debug, Parser)]
pub struct CommandReThumbnail {}

#[derive(Debug, Parser)]
pub struct CommandCompletions {}

#[derive(Debug, Parser)]
pub struct CommandCheckArchive {}
