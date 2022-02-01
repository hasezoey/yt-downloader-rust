//! Module for various Context traits

use std::path::Path;

/// Trait for Paths used by most top-level functions
pub trait BasePaths {
	/// Get the Output path of various commands
	/// For download, it would be something like "~/Music/ytdlr"
	fn get_output(&self) -> Option<&Path>;
	/// Get the temporary directory path to use for temporary stuff
	/// like intermediate downloads
	fn get_tmp(&self) -> Option<&Path>;
	/// Get the [`crate::archive_schema::Archive`] path to use
	fn get_archive(&self) -> Option<&Path>;
}

/// Options specific for the [`download`] function
/// TODO: replace link above to actual download function
pub trait DownloadOptions {
	/// Get if the output should be audio-only
	fn get_audio_only(&self) -> bool;
	/// Get if Re-Thumbnailing should be enabled
	fn get_re_thumbnail(&self) -> bool;
	/// Get if Cleanup should be done afterwards
	/// TODO: check if this is still needed, and if the name is correct
	fn get_cleanup(&self) -> bool;
	/// Get the URL to download
	fn get_url(&self) -> &str;
}

/// Options specific for the [`import`] function
/// Currently, there are no options specific for the "import" function
/// TODO: replace link above to actual import function
pub trait ImportOptions {}

/// Options speicifc for the [`verify-archive`] function
/// TODO: replace link above to actual verify-archive function
pub trait VerifyArchiveOptions {
	/// Get if the function should only check and not write / migrate
	fn get_dry_run(&self) -> bool;
}
