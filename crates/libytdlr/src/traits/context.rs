//! Module for various Context traits

use std::{
	ffi::OsStr,
	path::Path,
};

use diesel::SqliteConnection;

/// Trait for Paths used by most top-level functions
// pub trait BasePaths {
// 	/// Get the Output path of various commands
// 	/// For download, it would be something like "~/Music/ytdlr"
// 	fn get_output(&self) -> Option<&Path>;
// 	/// Get the temporary directory path to use for temporary stuff
// 	/// like intermediate downloads
// 	fn get_tmp(&self) -> Option<&Path>;
// 	/// Get the [`crate::archive_schema::Archive`] path to use
// 	fn get_archive(&self) -> Option<&Path>;
// }

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
	/// For example if [`crate::main::count::count`] was used, it should be those matched against the SQL Archive, and those existing in both should be output
	/// Otherwise, the whole SQL Archive could also be output, but may result in very large ytdl files
	fn gen_archive<'a>(&'a self, connection: &'a mut SqliteConnection)
		-> Option<Box<dyn Iterator<Item = String> + '_>>;
	/// Get the URL to download
	fn get_url<'a>(&'a self) -> &'a str;
	/// Get wheter or not to print out Command STDOUT (in this case ytdl)
	/// STDERR is always printed (using [`log`])
	/// With this returning `true`, the STDOUT output is also printed to [`log`], with [`log::trace`]
	fn print_command_stdout(&self) -> bool;
	/// Get a estimate of how many media elements will be downloaded
	/// This commonly should be the length of the vec containing [`crate::main::count::CountVideo`] returned from [`crate::main::count::count`]
	fn get_count_estimate(&self) -> usize;
}
