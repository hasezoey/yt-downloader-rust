//! Module for various Context traits

use std::{
	ffi::OsStr,
	path::Path,
};

use diesel::SqliteConnection;

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
		-> Option<Box<dyn Iterator<Item = String> + '_>>;
	/// Get the URL to download
	fn get_url(&self) -> &str;
	/// Get whether or not to print out Command STDOUT (in this case ytdl)
	/// STDERR is always printed (using [`log::trace`])
	/// With this returning `true`, the STDOUT output is also printed with [`log::trace`]
	fn print_command_stdout(&self) -> bool;
	/// Get a estimate of how many media elements will be downloaded in this run
	/// This could commonly be the playlist count that youtube-dl outputs
	/// if no count is available, a minimal count of 1 should be returned
	fn get_count_estimate(&self) -> usize;
	/// Get which subtitle languages to download
	/// see <https://github.com/yt-dlp/yt-dlp#subtitle-options> for what is available
	/// [None] disables adding subtitles
	fn sub_langs(&self) -> Option<&String>;
}
