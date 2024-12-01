//! Module for State Struct for all commands

use std::{
	ffi::OsString,
	path::PathBuf,
};

use libytdlr::{
	chrono,
	diesel,
	spawn::ytdl::YTDL_BIN_NAME,
	traits::download_options::DownloadOptions,
};
use once_cell::sync::Lazy;

use crate::clap_conf::{
	ArchiveMode,
	CommandDownload,
};

/// Struct to keep configuration data for the [`DownloadOptions`] trait
///
/// This data basically only contains non-chaning data between URLs
#[derive(Debug, PartialEq, Clone)]
pub struct DownloadState<'a> {
	/// Enable downloading / converting to audio only format
	audio_only_enable:       bool,
	/// Extra arguments to pass to ytdl
	extra_command_arguments: Vec<std::ffi::OsString>,
	/// Print youtube-dl stdout as trace logs
	print_command_log:       bool,
	/// Save youtube-dl logs to a file
	save_command_log:        bool,
	/// The Path to download to
	download_path:           PathBuf,

	/// Set which / how many entries of the archive are output to the youtube-dl archive
	archive_mode: ArchiveMode,

	/// Set the current URL to be downloaded
	current_url: String,
	/// Set which subtitle languages to download
	sub_langs:   Option<&'a String>,

	// Stores the youtube-dl version in use
	ytdl_version: libytdlr::chrono::NaiveDate,
}

/// The default youtube-dl version to use
static DEFAULT_YTDL_VERSION: Lazy<chrono::NaiveDate> =
	Lazy::new(|| return chrono::NaiveDate::from_ymd_opt(2023, 3, 4).unwrap());

/// The minimal youtube-dl that is recommended to be used
static MINIMAL_YTDL_VERSION: Lazy<chrono::NaiveDate> =
	Lazy::new(|| return chrono::NaiveDate::from_ymd_opt(2023, 3, 3).unwrap());

impl<'a> DownloadState<'a> {
	/// Create a new instance of [`DownloadState`] with the required options
	pub fn new(sub_args: &'a CommandDownload, download_path: PathBuf, ytdl_version: &str) -> Self {
		// process extra arguments into separated arguments of key and value (split once)
		let extra_cmd_args = sub_args
			.extra_ytdl_args
			.iter()
			.flat_map(|v| {
				if let Some((split1, split2)) = v.split_once(' ') {
					return Vec::from([OsString::from(split1), OsString::from(split2)]);
				}
				return Vec::from([OsString::from(v)]);
			})
			.collect();

		let ytdl_version = chrono::NaiveDate::parse_from_str(ytdl_version, "%Y.%m.%d").unwrap_or_else(|_| {
			warn!("Could not determine youtube-dl version properly, using default");

			return *DEFAULT_YTDL_VERSION;
		});

		if ytdl_version < *MINIMAL_YTDL_VERSION {
			warn!(
				"Used {} version ({}) is lower than the recommended {}",
				YTDL_BIN_NAME,
				ytdl_version.format("%Y.%m.%d"),
				MINIMAL_YTDL_VERSION.format("%Y.%m.%d"),
			);
		}

		return Self {
			audio_only_enable: sub_args.audio_only_enable,
			extra_command_arguments: extra_cmd_args,
			print_command_log: sub_args.print_youtubedl_log,
			save_command_log: sub_args.save_youtubedl_log,
			download_path,
			sub_langs: sub_args.sub_langs.as_ref(),

			archive_mode: sub_args.archive_mode,

			current_url: String::default(),
			ytdl_version,
		};
	}

	/// Set the current url ot be downloaded
	pub fn set_current_url<S: AsRef<str>>(&mut self, new_url: S) {
		// replace the already allocated string with the "new_url" without creating a new string
		self.current_url.replace_range(.., new_url.as_ref());
	}
}

impl DownloadOptions for DownloadState<'_> {
	fn audio_only(&self) -> bool {
		return self.audio_only_enable;
	}

	fn extra_ytdl_arguments(&self) -> Vec<&std::ffi::OsStr> {
		return self
			.extra_command_arguments
			.iter()
			.map(|v| return v.as_os_str())
			.collect();
	}

	fn download_path(&self) -> &std::path::Path {
		return self.download_path.as_path();
	}

	fn gen_archive<'a>(
		&'a self,
		connection: &'a mut diesel::SqliteConnection,
	) -> Option<Box<dyn Iterator<Item = String> + 'a>> {
		use diesel::prelude::*;
		use libytdlr::data::{
			sql_models::Media,
			sql_schema::media_archive,
		};

		if self.archive_mode == ArchiveMode::None {
			debug!("archive-mode is None, not outputting any ytdl archive");

			return Some(Box::new([].into_iter()));
		}

		// function to use to format all output to a youtube-dl archive, consistent across all options
		let fmtfn = |v: Result<libytdlr::data::sql_models::Media, diesel::result::Error>| {
			let v = v.ok()?;
			return Some(format!("{} {}\n", v.provider, v.media_id));
		};

		if self.archive_mode == ArchiveMode::All || self.archive_mode == ArchiveMode::Default {
			debug!("Dumping full sqlite archive as youtube-dl archive");

			let lines_iter = media_archive::dsl::media_archive
				.order(media_archive::_id.asc())
				// the following is some black-magic that rust-analyzer does not understand (no useful intellisense available)
				.load_iter::<Media, diesel::connection::DefaultLoadingMode>(connection)
				.ok()?
				// the following has some explicit type-annotation for the argument, because otherwise rust-analyzer does not provide any types
				.filter_map(fmtfn);

			return Some(Box::new(lines_iter));
		}

		// ArchiveMode::ByDate1000

		let lines_iter = media_archive::dsl::media_archive
			// order by newest to oldest
			.order(media_archive::inserted_at.desc())
			// limit this case to the newest 1000 media
			.limit(1000)
			.load_iter::<Media, diesel::connection::DefaultLoadingMode>(connection)
			.ok()?
			// the following has some explicit type-annotation for the argument, because otherwise rust-analyzer does not provide any types
			.filter_map(fmtfn);

		return Some(Box::new(lines_iter));
	}

	fn get_url(&self) -> &str {
		// check against "current_url" still being empty
		assert!(
			!self.current_url.is_empty(),
			"Expected \"current_url\" to not be empty at this point"
		);

		return &self.current_url;
	}

	fn print_command_log(&self) -> bool {
		return self.print_command_log;
	}

	fn save_command_log(&self) -> bool {
		return self.save_command_log;
	}

	fn sub_langs(&self) -> Option<&String> {
		return self.sub_langs;
	}

	fn ytdl_version(&self) -> chrono::NaiveDate {
		return self.ytdl_version;
	}
}

#[cfg(test)]
mod test {
	use super::*;

	// test that all static dates compile without problem
	#[test]
	fn static_dates_should_be_ok() {
		// simple test to test that the versions compile without panic
		let _ = *DEFAULT_YTDL_VERSION;
		let _ = *MINIMAL_YTDL_VERSION;

		// compare dates so that DEFAULT is always higher than MINIMAL
		assert!(*DEFAULT_YTDL_VERSION >= *MINIMAL_YTDL_VERSION);
	}
}
