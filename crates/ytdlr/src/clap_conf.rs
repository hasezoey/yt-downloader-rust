//! Module for Clap related structs (derived)

#![deny(missing_docs)] // comments are used for "--help" generation, so it should always be defined

use clap::{
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
#[clap(args_override_self(true))] // specifying a argument multiple times overwrites the earlier ones
#[clap(disable_help_subcommand(true))] // Disable subcommand "help", only "-h --help" should be used
#[clap(subcommand_negates_reqs(true))]
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
	#[must_use]
	pub fn custom_parse() -> Self {
		let mut parsed = Self::parse();

		Check::check(&mut parsed).expect("Expected the check to not fail"); // TODO: this should maybe be actually handled

		return parsed;
	}

	/// Get if the mode is interactive or not
	#[must_use]
	pub fn is_interactive(&self) -> bool {
		if self.explicit_tty.is_some() {
			return self.explicit_tty.expect("Should have failed with \"is_some\"");
		}

		return atty::is(atty::Stream::Stdout) && atty::is(atty::Stream::Stdin);
	}

	/// Get if the colors are enabled or not
	#[must_use]
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
	/// Re-Thumbnail specific files
	#[clap(alias = "rethumbnail")] // alias, otherwise only "re-thumbnail" would be valid
	ReThumbnail(CommandReThumbnail),
	// /// Generate all shell completions
	// Completions(CommandCompletions),
}

impl Check for SubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			SubCommands::Download(v) => return Check::check(v),
			SubCommands::Archive(v) => return Check::check(v),
			SubCommands::ReThumbnail(v) => return Check::check(v),
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
}

impl Check for ArchiveSubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			ArchiveSubCommands::Import(v) => return Check::check(v),
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

/// Run and download a given URL(s)
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandDownload {
	/// Audio Editor for audio files when using edits on post-processing
	#[clap(long, env = "YTDL_AUDIO_EDITOR")]
	pub audio_editor:              Option<PathBuf>,
	/// Video Editor for video files when using edits on post-processing
	#[clap(long, env = "YTDL_VIDEO_EDITOR")]
	pub video_editor:              Option<PathBuf>,
	/// Picard Path / Command to use
	#[clap(long = "picard", env = "YTDL_PICARD")]
	pub picard_editor:             Option<PathBuf>,
	/// Output path for any command that outputs a file
	#[clap(short, long, parse(from_os_str), env = "YTDL_OUT")]
	pub output_path:               Option<PathBuf>,
	/// Disable Re-Applying Thumbnails after a editor has run
	#[clap(long = "no-reapply-thumbnail", env = "YTDL_DISABLE_REAPPLY_THUMBNAIL")]
	pub reapply_thumbnail_disable: bool,
	/// Set download to be audio-only (if its not, it will just extract the audio)
	#[clap(short = 'a', long = "audio-only")]
	pub audio_only_enable:         bool,
	/// Force "gen_archive" to use the newest 1000 media elements instead of from count-result
	/// This may be useful if a playlist is meant to be processed, but has more than ~1000 elements
	#[clap(long = "force-genarchive-by-date")]
	pub force_genarchive_bydate:   bool,
	/// Force "gen_archive" to dump the full sqlite archive as a youtube-dl archive
	/// This may be useful for debugging or if you dont care about how big the youtube-dl archive gets
	#[clap(long = "force-genarchive-all")]
	pub force_genarchive_all:      bool,
	/// Print Youtube-DL stdout
	/// This will still require logging verbosity set to 3 or "RUST_LOG=trace"
	#[clap(long = "youtubedl-stdout")]
	pub print_youtubedl_stdout:    bool,
	/// Print Editor stdout (both video & audio)
	/// This will still require logging verbosity set to 3 or "RUST_LOG=trace"
	#[clap(long = "editor-stdout")]
	pub print_editor_stdout:       bool,

	pub urls: Vec<String>,
}

impl Check for CommandDownload {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Ok(());
	}
}

/// Manually run the Re-Apply Thumbnail step for a file with a specific image
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandReThumbnail {
	/// Input Image file to use as a Thumbnail (like a jpg)
	#[clap(short = 'i', long = "image", parse(from_os_str))]
	pub input_image_path:  PathBuf,
	/// Input Media file to apply a Thumbnail on (like a mp3)
	#[clap(short = 'm', long = "media", parse(from_os_str))]
	pub input_media_path:  PathBuf,
	/// Output path of the final file, by default it is the same as "media"
	#[clap(short = 'o', long = "out", parse(from_os_str))]
	pub output_media_path: Option<PathBuf>,
}

impl Check for CommandReThumbnail {
	fn check(&mut self) -> Result<(), crate::Error> {
		if self.output_media_path.is_none() {
			self.output_media_path = Some(self.input_media_path.clone());
		}

		return Ok(());
	}
}

// #[derive(Debug, Parser)]
// pub struct CommandCompletions {}

// impl Check for CommandCompletions {
// 	fn check(&mut self) -> Result<(), crate::Error> {
// 		todo!()
// 	}
// }

#[cfg(test)]
mod test {
	use super::*;

	mod command_download {
		use super::*;

		#[test]
		fn test_check() {
			let init_default = CommandDownload {
				audio_editor: None,
				output_path: None,
				video_editor: None,
				audio_only_enable: false,
				reapply_thumbnail_disable: false,
				urls: Vec::new(),
				force_genarchive_bydate: false,
				force_genarchive_all: false,
				print_youtubedl_stdout: false,
				print_editor_stdout: false,
				picard_editor: None,
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default, cloned);
		}
	}

	mod archive_import {
		use super::*;

		#[test]
		fn test_check() {
			let init_default = ArchiveImport {
				file_path: PathBuf::from("/hello"),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default, cloned);
		}
	}

	mod archive_subcommands {
		use super::*;

		#[test]
		fn test_check() {
			let init_default = ArchiveSubCommands::Import(ArchiveImport {
				file_path: PathBuf::from("/hello"),
			});

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default, cloned);
		}
	}

	mod archive_derive {
		use super::*;

		#[test]
		fn test_check() {
			let init_default_import = ArchiveDerive {
				subcommands: ArchiveSubCommands::Import(ArchiveImport {
					file_path: PathBuf::from("/hello"),
				}),
			};

			let mut cloned = init_default_import.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default_import, cloned);
		}
	}

	mod subcommands {
		use super::*;

		#[test]
		fn test_check() {
			{
				let init_default_download = SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				});

				let mut cloned = init_default_download.clone();
				assert!(cloned.check().is_ok());
				assert_eq!(init_default_download, cloned);
			}

			{
				let init_default_archive = SubCommands::Archive(ArchiveDerive {
					subcommands: ArchiveSubCommands::Import(ArchiveImport {
						file_path: PathBuf::from("/hello"),
					}),
				});

				let mut cloned = init_default_archive.clone();
				assert!(cloned.check().is_ok());
				assert_eq!(init_default_archive, cloned);
			}
		}
	}

	mod cli_derive {
		use super::*;

		#[test]
		fn test_check() {
			let init_default = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: None,
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default, cloned);
		}

		#[test]
		fn test_is_interactive_explicit() {
			let explicit_disable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(false),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			assert_eq!(false, explicit_disable.is_interactive());

			let explicit_enable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(true),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			assert_eq!(true, explicit_enable.is_interactive());
		}

		#[test]
		fn test_enable_colors_forced() {
			let explicit_disable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: None,
				force_color:  true,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			assert_eq!(true, explicit_disable.enable_colors());
		}

		#[test]
		fn test_enable_colors_forced_interactive() {
			let explicit_disable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(false),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			assert_eq!(false, explicit_disable.enable_colors());

			let explicit_enable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(true),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload {
					audio_editor: None,
					output_path: None,
					video_editor: None,
					audio_only_enable: false,
					reapply_thumbnail_disable: false,
					urls: Vec::new(),
					force_genarchive_bydate: false,
					force_genarchive_all: false,
					print_youtubedl_stdout: false,
					print_editor_stdout: false,
					picard_editor: None,
				}),
			};

			assert_eq!(true, explicit_enable.enable_colors());
		}
	}

	mod command_re_thumbnail {
		use super::*;

		#[test]
		fn test_check() {
			// initial value
			let mut init_default = CommandReThumbnail {
				input_image_path:  PathBuf::from("/hello/image.jpg"),
				input_media_path:  PathBuf::from("/hello/media.mp3"),
				output_media_path: None,
			};

			// clone initial value (to not have to duplicate it), for testing without running the function
			let cloned = {
				let mut tmp = init_default.clone();

				// modify the cloned value to what is expected
				tmp.output_media_path = Some(tmp.input_media_path.clone());

				tmp
			};

			// test to run the check and transform
			assert!(init_default.check().is_ok());
			// compare cloned manual and function execution
			assert_eq!(cloned, init_default);
		}
	}
}
