//! Module for State Struct for all commands

use std::{
	cell::Cell,
	path::{
		Path,
		PathBuf,
	},
};

use libytdlr::diesel;
use libytdlr::traits::context::DownloadOptions;

use crate::clap_conf::ArchiveMode;

/// Set the default count estimate
const DEFAULT_COUNT_ESTIMATE: usize = 1;

/// Struct to keep configuration data for the [`DownloadOptions`] trait
#[derive(Debug, PartialEq, Clone)]
pub struct DownloadState<'a> {
	/// Enable downloading / converting to audio only format
	audio_only_enable:       bool,
	/// Extra arguments to pass to ytdl
	extra_command_arguments: Vec<std::ffi::OsString>,
	/// Print youtube-dl stdout as trace logs
	print_stdout_debug:      bool,
	/// The Path to download to
	download_path:           PathBuf,
	/// Contains the value for the current playlist count estimate
	count_estimate:          Cell<usize>,

	/// Set which / how many entries of the archive are output to the youtube-dl archive
	archive_mode: ArchiveMode,

	/// Set the current URL to be downloaded
	current_url: String,
	/// Set which subtitle languages to download
	sub_langs:   Option<&'a String>,
}

impl<'a> DownloadState<'a> {
	/// Create a new instance of [`DownloadState`] with the required options
	pub fn new(
		audio_only_enable: bool,
		print_stdout_debug: bool,
		download_path: PathBuf,
		archive_mode: ArchiveMode,
		sub_langs: Option<&'a String>,
	) -> Self {
		return Self {
			audio_only_enable,
			// for now, there are no extra arguments supported
			extra_command_arguments: Vec::default(),
			print_stdout_debug,
			count_estimate: Cell::new(DEFAULT_COUNT_ESTIMATE),
			download_path,
			sub_langs,

			archive_mode,

			current_url: String::default(),
		};
	}

	/// Set the current url ot be downloaded
	pub fn set_current_url<S: AsRef<str>>(&mut self, new_url: S) {
		// replace the already allocated string with the "new_url" without creating a new string
		self.current_url.replace_range(.., new_url.as_ref());
	}

	/// Set "count_result" for generating the archive and for "get_count_estimate"
	pub fn set_count_estimate(&self, count: usize) {
		if count < DEFAULT_COUNT_ESTIMATE {
			self.count_estimate.replace(DEFAULT_COUNT_ESTIMATE);
		} else {
			self.count_estimate.replace(count);
		}
	}

	/// Get the "download_path" stored in the instance as reference
	pub fn get_download_path(&self) -> &Path {
		return &self.download_path;
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
	) -> Option<Box<dyn Iterator<Item = String> + '_>> {
		use diesel::prelude::*;
		use libytdlr::data::{
			sql_models::*,
			sql_schema::*,
		};

		if self.archive_mode == ArchiveMode::None {
			debug!("force_no_archive, not outputting any ytdl archive");

			return Some(Box::new([].into_iter()));
		}

		// function to use to format all output to a youtube-dl archive, consistent across all options
		let fmfn = |v: Result<libytdlr::data::sql_models::Media, diesel::result::Error>| {
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
				.filter_map(fmfn);

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
			.filter_map(fmfn);

		return Some(Box::new(lines_iter));
	}

	fn get_url(&self) -> &str {
		// check against "current_url" still being empty
		if self.current_url.is_empty() {
			panic!("Expected \"current_url\" to not be empty at this point");
		}

		return &self.current_url;
	}

	fn print_command_stdout(&self) -> bool {
		return self.print_stdout_debug;
	}

	fn get_count_estimate(&self) -> usize {
		return self.count_estimate.get();
	}

	fn sub_langs(&self) -> Option<&String> {
		return self.sub_langs;
	}
}
