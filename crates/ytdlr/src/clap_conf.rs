//! Module for Clap related structs (derived)

#![deny(missing_docs)] // comments are used for "--help" generation, so it should always be defined

use clap::{
	ArgAction,
	Parser,
	Subcommand,
	ValueEnum,
};
use clap_complete::Shell;
use is_terminal::IsTerminal;
use std::{
	collections::HashSet,
	error::Error,
	fmt::Display,
	path::PathBuf,
	str::FromStr,
};

/// Trait to check and transform all Command Structures
trait Check {
	/// Check and transform self to be correct
	fn check(&mut self) -> Result<(), crate::Error>;
}

#[derive(Debug, Parser, Clone, PartialEq)]
#[command(author, version, about, version = env!("YTDLR_VERSION"), long_about = None)]
#[command(bin_name("ytdlr"))]
#[command(disable_help_subcommand(true))] // Disable subcommand "help", only "-h" or "--help" should be used
#[command(subcommand_negates_reqs(true))]
pub struct CliDerive {
	/// Set Loggin verbosity (0 - Default - WARN, 1 - INFO, 2 - DEBUG, 3 - TRACE)
	#[arg(short, long, action = ArgAction::Count, env = "YTDL_VERBOSITY")]
	pub verbosity:    u8,
	/// Temporary directory path to store intermediate files (like downloaded files before being moved)
	#[arg(long = "tmp", env = "YTDL_TMP")]
	pub tmp_path:     Option<PathBuf>,
	/// Request vscode lldb debugger before continuing to execute.
	/// Only available in debug target
	#[arg(long)]
	#[cfg(debug_assertions)]
	pub debugger:     bool,
	/// Archive path to use, if a archive should be used
	#[arg(long = "archive", env = "YTDL_ARCHIVE")]
	pub archive_path: Option<PathBuf>,
	/// Explicitly set interactive / not interactive
	#[arg(long = "interactive")]
	pub explicit_tty: Option<bool>,
	/// Force Color to be active in any mode
	#[arg(long = "color")]
	pub force_color:  bool,

	#[command(subcommand)]
	pub subcommands: SubCommands,
}

impl CliDerive {
	/// Execute [clap::Parser::parse] and apply custom validation and transformation logic
	pub fn custom_parse() -> Result<Self, crate::Error> {
		let mut parsed = Self::parse();

		Check::check(&mut parsed)?;

		return Ok(parsed);
	}

	/// Get if the mode is interactive or not
	#[must_use]
	pub fn is_interactive(&self) -> bool {
		if let Some(v) = self.explicit_tty {
			return v;
		}

		return std::io::stdout().is_terminal() && std::io::stdin().is_terminal();
	}

	/// Get if the colors are enabled or not
	#[must_use]
	pub fn enable_colors(&self) -> bool {
		return self.force_color | self.is_interactive();
	}

	/// Get if debug is enabled
	/// Only able to be "true" in "debug" target
	#[must_use]
	pub fn debugger_enabled(&self) -> bool {
		#[cfg(debug_assertions)]
		return self.debugger;
		#[cfg(not(debug_assertions))]
		return false;
	}
}

impl Check for CliDerive {
	fn check(&mut self) -> Result<(), crate::Error> {
		// apply "expand_tilde" to archive_path
		self.archive_path = match self.archive_path.take() {
			// this has to be so round-about, because i dont know of a function that would allow functionality like "and_then" but instead of returning the same value, it would return a result
			Some(v) => Some(crate::utils::fix_path(v).ok_or_else(|| {
				return crate::Error::other("Archive Path was provided, but could not be expanded / fixed");
			})?),
			None => None,
		};

		// apply "expand_tilde" to archive_path
		self.tmp_path = match self.tmp_path.take() {
			// this has to be so round-about, because i dont know of a function that would allow functionality like "and_then" but instead of returning the same value, it would return a result
			Some(v) => Some(crate::utils::fix_path(v).ok_or_else(|| {
				return crate::Error::other("Temp Path was provided, but could not be expanded / fixed");
			})?),
			None => None,
		};

		return Check::check(&mut self.subcommands);
	}
}

#[derive(Debug, Subcommand, Clone, PartialEq)]
pub enum SubCommands {
	/// The main purpose of the binary, download a URL(s)
	Download(CommandDownload),
	/// Archive Managing Commands
	Archive(ArchiveDerive),
	/// Re-Thumbnail specific files
	#[command(alias = "rethumbnail")] // alias, otherwise only "re-thumbnail" would be the only valid option
	ReThumbnail(CommandReThumbnail),
	/// Generate shell completions
	Completions(CommandCompletions),
	/// Unicode Terminal testing options
	#[cfg(debug_assertions)]
	#[command(name = "unicode-test")]
	UnicodeTerminalTest(CommandUnicodeTerminalTest),
}

impl Check for SubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			SubCommands::Download(v) => return Check::check(v),
			SubCommands::Archive(v) => return Check::check(v),
			SubCommands::ReThumbnail(v) => return Check::check(v),
			SubCommands::Completions(v) => return Check::check(v),
			#[cfg(debug_assertions)]
			SubCommands::UnicodeTerminalTest(v) => return Check::check(v),
		}
	}
}

#[derive(Debug, Parser, Clone, PartialEq)]
pub struct ArchiveDerive {
	#[command(subcommand)]
	pub subcommands: ArchiveSubCommands,
}

impl Check for ArchiveDerive {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Check::check(&mut self.subcommands);
	}
}

#[derive(Debug, Subcommand, Clone, PartialEq)]
pub enum ArchiveSubCommands {
	/// Import a Archive file, be it youtube-dl, ytdlr-json, or ytdlr-sqlite
	Import(ArchiveImport),
	/// Search the Archive
	Search(ArchiveSearch),
}

impl Check for ArchiveSubCommands {
	fn check(&mut self) -> Result<(), crate::Error> {
		match self {
			ArchiveSubCommands::Import(v) => return Check::check(v),
			ArchiveSubCommands::Search(v) => return Check::check(v),
		}
	}
}

/// Import a Archive into the current Archive
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct ArchiveImport {
	/// The Archive file to import from
	pub file_path: PathBuf,
}

impl Check for ArchiveImport {
	fn check(&mut self) -> Result<(), crate::Error> {
		// apply "expand_tilde" to archive_path
		self.file_path = crate::utils::fix_path(&self.file_path).ok_or_else(|| {
			return crate::Error::other("Import Path was provided, but could not be expanded / fixed");
		})?;

		return Ok(());
	}
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Copy)]
#[value(rename_all = "camelCase")]
pub enum ArchiveSearchColumn {
	/// For the SQL column "provider"
	Provider,
	/// For the SQL column "media_id"
	MediaId,
	/// For the SQL column "title"
	Title,
	/// For the SQL column "inserted_at"
	InsertedAt,
}

impl Display for ArchiveSearchColumn {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		return write!(
			f,
			"{}",
			match *self {
				ArchiveSearchColumn::Provider => "Provider",
				ArchiveSearchColumn::MediaId => "MediaId",
				ArchiveSearchColumn::InsertedAt => "InsertedAt",
				ArchiveSearchColumn::Title => "Title",
			}
		);
	}
}

impl FromStr for ArchiveSearchColumn {
	type Err = crate::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		return Ok(match s.to_lowercase().as_str() {
			"provider" => Self::Provider,
			"mediaid"
			// may be confused with the row-id
			| "id" => Self::MediaId,
			"insertedat"
			| "inserted" => Self::InsertedAt,
			"title" => Self::Title,
			_ => return Err(crate::Error::other(format!("Unknown column \"{}\"", s))),
		});
	}
}

/// Parse a key-value pair from the input
/// from <https://github.com/clap-rs/clap/blob/78bb48b6b8ef4d597b4b30b9add7927a2b0b0d8d/examples/typed-derive.rs#L48-L59>
fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
	T: std::str::FromStr,
	T::Err: Error + Send + Sync + 'static,
	U: std::str::FromStr,
	U::Err: Error + Send + Sync + 'static,
{
	let pos = s
		.find('=')
		.ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
	return Ok((s[..pos].parse()?, s[pos + 1..].parse()?));
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Copy)]
#[value(rename_all = "camelCase")]
#[allow(clippy::upper_case_acronyms)]
pub enum SearchResultFormat {
	/// Output as: `[provider:media_id] [inserted_at] title`
	Normal,
	/// Output as CSV, Command delimited
	CSVC,
	/// Output as CSV, Tab delimited
	CSVT,
}

/// Search the Archive
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct ArchiveSearch {
	/// Query a column with the given search terms, supported columns are (values in parenthesis are aliases):
	///   Provider, Title, MediaId(id), InsertedAt(inserted)
	/// columns can be anycase
	/// Examples:
	///   "title=some good title"
	///   title=sometitle
	///   title="long title"
	///   "inserted=>=2023-05"
	/// Supported Date operators are (omitted defaults to "="):
	///   >,<,=,>=,<=
	#[arg(required(true), value_parser = parse_key_val::<ArchiveSearchColumn, String>, verbatim_doc_comment)]
	pub queries: Vec<(ArchiveSearchColumn, String)>,

	/// Set the limit of returned values
	#[arg(short = 'l', long = "limit", default_value_t = 10)]
	pub limit: i64,

	/// Set which return format should be used
	#[arg(short = 'f', long = "result-format", value_enum, default_value_t=SearchResultFormat::Normal)]
	pub result_format: SearchResultFormat,
}

impl Check for ArchiveSearch {
	fn check(&mut self) -> Result<(), crate::Error> {
		// check that a query for a column is only defined once
		let mut map = HashSet::new();

		for val in &self.queries {
			if map.contains(&val.0.to_string()) {
				return Err(crate::Error::other(format!(
					"A column query can only be defined once, found duplicate for \"{}\"",
					val.0
				)));
			}
			map.insert(val.0.to_string());
		}

		return Ok(());
	}
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Copy)]
#[value(rename_all = "camelCase")]
pub enum ArchiveMode {
	/// Use the default Archive-Mode, currently corresponds to "all"
	Default,
	/// Dump the full SQLite archive as a youtube-dl archive
	All,
	/// Output the newest 1000 media elements from the archive
	ByDate1000,
	/// Dont add any entries from the SQLite archive to the youtube-dl archive
	/// This does not disable youtube-dl archive generation
	/// This also does not disable adding entries to the SQLite archive
	None,
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Copy)]
#[value(rename_all = "camelCase")]
pub enum DownloadSkipWith {
	/// Corresponds to "n"
	Skip,
	/// Corresponds to "y"
	Edit,
	/// Corresponds to "a"
	AudioEdit,
	/// Corresponds to "v"
	VideoEdit,
}

impl Default for ArchiveMode {
	fn default() -> Self {
		return Self::Default;
	}
}

/// Run and download a given URL(s)
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandDownload {
	/// Audio Editor for audio files when using edits on post-processing
	/// Must be either a absolute path or findable via PATH
	#[arg(long, env = "YTDL_AUDIO_EDITOR")]
	pub audio_editor:              Option<PathBuf>,
	/// Video Editor for video files when using edits on post-processing
	/// Must be either a absolute path or findable via PATH
	#[arg(long, env = "YTDL_VIDEO_EDITOR")]
	pub video_editor:              Option<PathBuf>,
	/// Tagger Path / Command to use
	/// Must be either a absolute path or findable via PATH
	#[arg(long = "tagger", env = "YTDL_TAGGER")]
	pub tagger_editor:             Option<PathBuf>,
	/// Media player Command to use
	/// Must be either a absolute path or findable via PATH
	#[arg(long = "player", env = "YTDL_PLAYER")]
	pub player_editor:             Option<PathBuf>,
	/// Output path for any command that outputs a file
	#[arg(short, long, env = "YTDL_OUT")]
	pub output_path:               Option<PathBuf>,
	/// Disable Re-Applying Thumbnails after a editor has run
	#[arg(long = "no-reapply-thumbnail", env = "YTDL_DISABLE_REAPPLY_THUMBNAIL")]
	pub reapply_thumbnail_disable: bool,
	/// Set download to be audio-only (if its not, it will just extract the audio)
	#[arg(short = 'a', long = "audio-only")]
	pub audio_only_enable:         bool,
	/// Set which entries should be output to the youtube-dl archive
	/// This does not affect entries being added to the SQLite archive
	#[arg(long = "archive-mode", value_enum, default_value_t=ArchiveMode::default())]
	pub archive_mode:              ArchiveMode,
	/// Print Youtube-DL log
	/// This will still require logging verbosity set to 3 or "RUST_LOG=trace"
	#[arg(long = "youtubedl-log")]
	pub print_youtubedl_log:       bool,
	/// Save Youtube-DL logs to a file
	/// File will be in the temporary directory, named "yt-dl_PID.log" where the PID is the ytdlr's pid
	#[arg(long = "youtubedl-logfile")]
	pub save_youtubedl_log:        bool,
	/// Disables allowing 0 URL's to just check the recovery
	#[arg(long = "no-check-recovery")]
	pub no_check_recovery:         bool,
	/// Set to automatically open the tagger in the end
	/// also overwrites the default option of moving for non-interactive mode
	#[arg(long = "open-tagger")]
	pub open_tagger:               bool,
	/// Apply a single action to all edit-media
	#[arg(long = "skip-with", value_enum)]
	pub skip_with:                 Option<DownloadSkipWith>,
	/// Set which subtitle languages to download
	/// see <https://github.com/yt-dlp/yt-dlp#subtitle-options>
	#[arg(long = "sub-langs", env = "YTDL_SUB_LANGS")]
	pub sub_langs:                 Option<String>,
	/// Add extra arguments to the ytdl command, requires usage of "="
	/// Example: --extra-ytdl-args="--max-downloads 10"
	#[arg(long = "extra-ytdl-args")]
	pub extra_ytdl_args:           Vec<String>,

	pub urls: Vec<String>,
}

impl Check for CommandDownload {
	fn check(&mut self) -> Result<(), crate::Error> {
		// apply "expand_tilde" to archive_path
		self.output_path = match self.output_path.take() {
			// this has to be so round-about, because i dont know of a function that would allow functionality like "and_then" but instead of returning the same value, it would return a result
			Some(v) => Some(crate::utils::fix_path(v).ok_or_else(|| {
				return crate::Error::other("Output Path was provided, but could not be expanded / fixed");
			})?),
			None => None,
		};

		return Ok(());
	}
}

// Simple default implementation for testing use only
#[cfg(test)]
impl Default for CommandDownload {
	fn default() -> Self {
		return Self {
			audio_editor: None,
			output_path: None,
			video_editor: None,
			audio_only_enable: false,
			reapply_thumbnail_disable: false,
			urls: Vec::new(),
			archive_mode: ArchiveMode::Default,
			print_youtubedl_log: false,
			save_youtubedl_log: false,
			tagger_editor: None,
			no_check_recovery: false,
			open_tagger: false,
			sub_langs: None,
			player_editor: None,
			extra_ytdl_args: Vec::new(),
			skip_with: None,
		};
	}
}

/// Manually run the Re-Apply Thumbnail step for a file with a specific image
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandReThumbnail {
	/// Input Image file to use as a Thumbnail (like a jpg)
	#[arg(short = 'i', long = "image")]
	pub input_image_path:  PathBuf,
	/// Input Media file to apply a Thumbnail on (like a mp3)
	#[arg(short = 'm', long = "media")]
	pub input_media_path:  PathBuf,
	/// Output path of the final file, by default it is the same as "media"
	#[arg(short = 'o', long = "out")]
	pub output_media_path: Option<PathBuf>,
}

impl Check for CommandReThumbnail {
	fn check(&mut self) -> Result<(), crate::Error> {
		// apply "expand_tilde" to archive_path
		self.input_image_path = crate::utils::fix_path(&self.input_image_path).ok_or_else(|| {
			return crate::Error::other("Input Image Path was provided, but could not be expanded / fixed");
		})?;

		// apply "expand_tilde" to archive_path
		self.input_media_path = crate::utils::fix_path(&self.input_media_path).ok_or_else(|| {
			return crate::Error::other("Input Media Path was provided, but could not be expanded / fixed");
		})?;

		// apply "expand_tilde" to archive_path
		self.output_media_path = match self.output_media_path.take() {
			// this has to be so round-about, because i dont know of a function that would allow functionality like "and_then" but instead of returning the same value, it would return a result
			Some(v) => Some(crate::utils::fix_path(v).ok_or_else(|| {
				return crate::Error::other("Output Media Path was provided, but could not be expanded / fixed");
			})?),
			None => None,
		};

		// the "fix_path" is done above, to not have to seperate / repeat the processing
		if self.output_media_path.is_none() {
			self.output_media_path = Some(self.input_media_path.clone());
		}

		return Ok(());
	}
}

#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandCompletions {
	/// Set which shell completions should be generated
	/// Supported are: Bash, Elvish, Fish, PowerShell, Zsh
	#[arg(short = 's', long = "shell", value_enum)]
	pub shell:            Shell,
	/// Output path where to output the completions to
	/// Not specifying this will print to STDOUT
	#[arg(short = 'o', long = "out")]
	pub output_file_path: Option<PathBuf>,
}

impl Check for CommandCompletions {
	fn check(&mut self) -> Result<(), crate::Error> {
		// apply "expand_tilde" to archive_path
		self.output_file_path = match self.output_file_path.take() {
			// this has to be so round-about, because i dont know of a function that would allow functionality like "and_then" but instead of returning the same value, it would return a result
			Some(v) => Some(crate::utils::fix_path(v).ok_or_else(|| {
				return crate::Error::other("Output Media Path was provided, but could not be expanded / fixed");
			})?),
			None => None,
		};

		return Ok(());
	}
}

/// Unicode Terminal Testing options
#[cfg(debug_assertions)]
#[derive(Debug, Parser, Clone, PartialEq)]
pub struct CommandUnicodeTerminalTest {
	/// Print full `msg_to_cluster` vec
	#[arg(short = 'c', long = "content")]
	pub print_content: bool,
	/// The string to test
	pub string:        String,
}

#[cfg(debug_assertions)]
impl Check for CommandUnicodeTerminalTest {
	fn check(&mut self) -> Result<(), crate::Error> {
		return Ok(());
	}
}

// the following tests make use of environment variables (explicitly and implicitly), and may conflict with eachother
#[cfg(test)]
mod test {
	use super::*;
	use std::path::Path;

	mod command_download {
		use super::*;

		// basic test that CommandDownload correctly parses and that Check returns a ok result
		#[test]
		fn test_basic_parse_and_check() {
			// the following is not quite a "clean" state, because "parse_from" still looks at environment variables
			let original = CommandDownload::parse_from([""].iter());

			let mut cloned = original.clone();
			assert!(cloned.check().is_ok()); // should always be successful, regardless of if "parse_from" looks at the environment variables or not
			assert_eq!(original, cloned);
		}

		#[test]
		fn test_check_outpath_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			let mut original = CommandDownload {
				output_path: Some(PathBuf::from("~/somedir")),
				..Default::default()
			};

			// check that the original does not have the path fixed yet
			assert_eq!(original.output_path, Some(PathBuf::from("~/somedir")));

			let mut cloned = original.clone();
			assert!(cloned.check().is_ok());

			// manually fix the original
			original.output_path = Some(homedir.join("somedir"));
			// check that the cloned(auto fixed) version matches what the manual fix is doing
			assert_eq!(original, cloned);
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

		#[test]
		fn test_check_filepath_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			let mut init_default = ArchiveImport {
				file_path: PathBuf::from("~/somedir"),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());

			// manually fix in the init
			init_default.file_path = homedir.join("somedir");
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
				let init_default_download = SubCommands::Download(CommandDownload::default());

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
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());
			assert_eq!(init_default, cloned);
		}

		#[test]
		fn test_check_archivepath_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			let mut init_default = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: Some(PathBuf::from("~/somedir")),
				explicit_tty: None,
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());

			// manually fix in the init
			init_default.archive_path = Some(homedir.join("somedir"));
			assert_eq!(init_default, cloned);
		}

		#[test]
		fn test_check_tmppath_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			let mut init_default = CliDerive {
				verbosity:    0,
				tmp_path:     Some(PathBuf::from("~/somedir")),
				debugger:     false,
				archive_path: None,
				explicit_tty: None,
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			let mut cloned = init_default.clone();
			assert!(cloned.check().is_ok());

			// manually fix in the init
			init_default.tmp_path = Some(homedir.join("somedir"));
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
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			assert!(!explicit_disable.is_interactive());

			let explicit_enable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(true),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			assert!(explicit_enable.is_interactive());
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
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			assert!(explicit_disable.enable_colors());
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
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			assert!(!explicit_disable.enable_colors());

			let explicit_enable = CliDerive {
				verbosity:    0,
				tmp_path:     None,
				debugger:     false,
				archive_path: None,
				explicit_tty: Some(true),
				force_color:  false,
				subcommands:  SubCommands::Download(CommandDownload::default()),
			};

			assert!(explicit_enable.enable_colors());
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

			let mut cloned = init_default.clone();
			// test to run the check and transform
			assert!(cloned.check().is_ok());
			// manually fix in the init
			init_default.output_media_path = Some(init_default.input_media_path.clone());
			// compare cloned manual and function execution
			assert_eq!(cloned, init_default);
		}

		#[test]
		fn test_check_inputpaths_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			// initial value
			let mut init_default = CommandReThumbnail {
				input_image_path:  PathBuf::from("~/image.jpg"),
				input_media_path:  PathBuf::from("~/media.mp3"),
				output_media_path: None,
			};

			let mut cloned = init_default.clone();
			// test to run the check and transform
			assert!(cloned.check().is_ok());

			// manually fix in the init
			init_default.input_image_path = homedir.join("image.jpg");
			init_default.input_media_path = homedir.join("media.mp3");
			init_default.output_media_path = Some(init_default.input_media_path.clone());
			// compare cloned manual and function execution
			assert_eq!(init_default, cloned);
		}

		#[test]
		fn test_check_outputpaths_fixed() {
			// fake home
			let homedir = Path::new("/custom/home");
			std::env::set_var("HOME", homedir);

			// initial value
			let mut init_default = CommandReThumbnail {
				input_image_path:  PathBuf::from("~/image.jpg"),
				input_media_path:  PathBuf::from("~/media.mp3"),
				output_media_path: Some(PathBuf::from("~/out.mp3")),
			};

			let mut cloned = init_default.clone();
			// test to run the check and transform
			assert!(cloned.check().is_ok());

			// manually fix in the init
			init_default.input_image_path = homedir.join("image.jpg");
			init_default.input_media_path = homedir.join("media.mp3");
			init_default.output_media_path = Some(homedir.join("out.mp3"));
			// compare cloned manual and function execution
			assert_eq!(init_default, cloned);
		}
	}
}
