//! Module for Clap related structs (derived)

#![deny(missing_docs)] // comments are used for "--help" generation, so it should always be defined

use clap::{
	AppSettings,
	Parser,
	Subcommand,
};
use std::path::PathBuf;

/// Trait to check and transform all Command Structures
trait Check {
	/// Check and transform self to be correct
	fn check(&mut self) -> Result<(), crate::Error>;
}

#[derive(Debug, Parser, Clone, PartialEq)]
#[clap(author, version, about, long_about = None)]
#[clap(bin_name("ytdlr"))]
#[clap(global_setting(AppSettings::AllArgsOverrideSelf))] // specifying a argument multiple times overwrites the earlier ones
#[clap(global_setting(AppSettings::DisableHelpSubcommand))] // Disable subcommand "help", only "-h --help" should be used
#[clap(global_setting(AppSettings::SubcommandsNegateReqs))]
pub struct CliDerive {
	/// Set Loggin verbosity (0 - Default - WARN, 1 - INFO, 2 - DEBUG, 3 - TRACE)
	#[clap(short, long, parse(from_occurrences), env = "YTDL_VERBOSITY")]
	pub verbosity:    u8,
	/// Temporary directory path to store intermediate files (like downloaded files before being processed)
	#[clap(long = "tmp", parse(from_os_str), env = "YTDL_TMP")]
	pub tmp_path:     Option<PathBuf>,
	/// Request vscode lldb debugger before continuing to execute
	#[clap(long)]
	pub debugger:     bool,
	/// Archive path to use, if a archive should be used
	#[clap(long = "archive", parse(from_os_str), env = "YTDL_ARCHIVE")]
	pub archive_path: Option<PathBuf>,
	/// Explicitly set interactive / not interactive
	#[clap(long = "interactive")]
	pub explicit_tty: Option<bool>,
	/// Force Color to be active in any mode
	#[clap(long = "color")]
	pub force_color:  bool,

	#[clap(subcommand)]
	pub subcommands: SubCommands,
}

impl CliDerive {
	/// Execute clap::Parser::parse and apply custom validation and transformation logic
	pub fn custom_parse() -> Self {
		let mut parsed = Self::parse();

		Check::check(&mut parsed).expect("Expected the check to not fail"); // TODO: this should maybe be actually handled

		return parsed;
	}

	/// Get if the mode is interactive or not
	pub fn is_interactive(&self) -> bool {
		if self.explicit_tty.is_some() {
			return self.explicit_tty.expect("Should have failed with \"is_some\"");
		}

		return atty::is(atty::Stream::Stdout) && atty::is(atty::Stream::Stdin);
	}

	/// Get if the colors are enabled or not
	pub fn enable_colors(&self) -> bool {
		return self.force_color | self.is_interactive();
	}
}

impl Check for CliDerive {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Check::check(&mut self.subcommands);
	}
}

#[derive(Debug, Subcommand, Clone, PartialEq)]
pub enum SubCommands {
	/// The main purpose of the binary, download URL
	Download(CommandDownload),
	/// Archive Managing Commands
	Archive(ArchiveDerive),
	// /// Re-Thumbnail specific files
	// ReThumbnail(CommandReThumbnail),
	// /// Generate all shell completions
	// Completions(CommandCompletions),
}

impl Check for SubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			SubCommands::Download(v) => return Check::check(v),
			SubCommands::Archive(v) => return Check::check(v),
			// SubCommands::ReThumbnail(v) => return Check::check(v),
			// SubCommands::Completions(v) => return Check::check(v),
		}
	}
}

#[derive(Debug, Parser, Clone, PartialEq)]
pub struct ArchiveDerive {
	#[clap(subcommand)]
	pub subcommands: ArchiveSubCommands,
}

impl Check for ArchiveDerive {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Check::check(&mut self.subcommands);
	}
}

#[derive(Debug, Subcommand, Clone, PartialEq)]
pub enum ArchiveSubCommands {
	/// Import a Archive file, be it youtube-dl, ytdlr-json
	Import(ArchiveImport),
	// /// Migrate and check the current Archive
	// Migrate(ArchiveMigrate),
}

impl Check for ArchiveSubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			ArchiveSubCommands::Import(v) => return Check::check(v),
			// ArchiveSubCommands::Migrate(v) => return Check::check(v),
		}
	}
}

/// Import a Archive into the current Archive
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct ArchiveImport {
	/// The Archive file to import from
	#[clap(parse(from_os_str))]
	pub file_path: PathBuf,
}

impl Check for ArchiveImport {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Ok(());
	}
}

// /// Migrate and check the current Archive in use
// #[derive(Debug, Parser)]
// pub struct ArchiveMigrate {
// 	/// Check the current Archive for any problems
// 	/// This includes: old archive format, unapplied migrations, not existing, broken archive
// 	#[clap(long)]
// 	pub check: bool,
// }

// impl Check for ArchiveMigrate {
// 	fn check(&mut self) -> Result<(), crate::Error> {
// 		return Ok(());
// 	}
// }

/// Run and download a given URL(s)
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandDownload {
	/// Audio Editor for audio files when using edits on post-processing
	#[clap(long, env = "YTDL_AUDIO_EDITOR")]
	pub audio_editor:              Option<PathBuf>,
	/// Video Editor for video files when using edits on post-processing
	#[clap(long, env = "YTDL_VIDEO_EDITOR")]
	pub video_editor:              Option<PathBuf>,
	/// Output path for any command that outputs a file
	#[clap(short, long, parse(from_os_str), env = "YTDL_OUT")]
	pub output_path:               Option<PathBuf>,
	/// Disable Re-Applying Thumbnails after a editor has run
	#[clap(long = "no-reapply-thumbnail", env = "YTDL_DISABLE_REAPPLY_THUMBNAIL")]
	pub reapply_thumbnail_disable: bool,
	/// Set download to be audio-only (if its not, it will just extract the audio)
	#[clap(short = 'a', long = "audio-only")]
	pub audio_only_enable:         bool,

	pub urls: Vec<String>,
}

impl Check for CommandDownload {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Ok(());
	}
}

// /// Manually run the Re-Apply Thumbnail step for a file with a specific image
// #[derive(Debug, Parser)]
// pub struct CommandReThumbnail {
// 	/// Input Image file to use as a Thumbnail (like a jpg)
// 	#[clap(short = 'i', long = "image", parse(from_os_str))]
// 	pub input_image_path:  PathBuf,
// 	/// Input Media file to apply a Thumbnail on (like a mp3)
// 	#[clap(short = 'm', long = "media", parse(from_os_str))]
// 	pub input_media_path:  PathBuf,
// 	/// Output path of the final file, by default it is the same as "media"
// 	#[clap(short = 'o', long = "out", parse(from_os_str))]
// 	pub output_media_path: Option<PathBuf>,
// }

// impl Check for CommandReThumbnail {
// 	fn check(&mut self) -> Result<(), crate::Error> {
// 		if self.output_media_path.is_none() {
// 			self.output_media_path = Some(self.input_media_path.clone());
// 		}

// 		return Ok(());
// 	}
// }

// #[derive(Debug, Parser)]
// pub struct CommandCompletions {}

// impl Check for CommandCompletions {
// 	fn check(&mut self) -> Result<(), crate::Error> {
// 		todo!()
// 	}
// }
