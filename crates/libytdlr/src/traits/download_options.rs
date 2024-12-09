//! Module for various Context traits

use std::{
	ffi::OsStr,
	path::Path,
};

use diesel::SqliteConnection;

/// The Format argument to use for the command.
///
/// See [yt-dlp Post-Processing Options](https://github.com/yt-dlp/yt-dlp?tab=readme-ov-file#post-processing-options) `--remux-video`
/// for possible rules.
pub type FormatArgument<'a> = &'a str;

/// Options specific for the [`crate::main::download::download_single`] function
pub trait DownloadOptions {
	/// Get if the "audio-only" flag should be added
	fn audio_only(&self) -> bool;

	/// Get Extra Arguments that should be added to the ytdl command
	fn extra_ytdl_arguments(&self) -> Vec<&OsStr>;

	/// Get the path to where the Media should be downloaded to
	fn download_path(&self) -> &Path;

	/// Get a iterator over all the lines for a ytdl archive
	/// All required videos should be made available with this function
	fn gen_archive<'a>(&'a self, connection: &'a mut SqliteConnection)
		-> Option<Box<dyn Iterator<Item = String> + 'a>>;

	/// Get the URL to download
	fn get_url(&self) -> &str;

	/// Get whether or not to print out Command STDOUT & STDERR (in this case ytdl)
	/// STDERR is always printed (using [`log::trace`])
	/// With this returning `true`, the STDOUT output is also printed with [`log::trace`]
	fn print_command_log(&self) -> bool;

	/// Get whether or not to save the Command STDOUT & STDERR to a file in the temporary directory
	fn save_command_log(&self) -> bool;

	/// Get which subtitle languages to download
	/// see <https://github.com/yt-dlp/yt-dlp#subtitle-options> for what is available
	/// [None] disables adding subtitles
	fn sub_langs(&self) -> Option<&String>;

	/// Get the current youtube-dl version in use as a chrono date
	fn ytdl_version(&self) -> chrono::NaiveDate;

	/// Get the format for audio-only/audio-extract downloads
	///
	/// Only set extensions supported by youtube-dl
	fn get_audio_format(&self) -> FormatArgument;

	/// Get the format for video downloads
	///
	/// Only set extensions supported by youtube-dl
	fn get_video_format(&self) -> FormatArgument;
}
