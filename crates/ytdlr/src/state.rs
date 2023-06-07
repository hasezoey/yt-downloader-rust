//! Module for State Struct for all commands

use std::{
	cell::Cell,
	ffi::OsString,
	path::PathBuf,
};

use libytdlr::{
	diesel,
	traits::download_options::DownloadOptions,
};

use crate::clap_conf::ArchiveMode;

/// Set the default count estimate
const DEFAULT_COUNT_ESTIMATE: usize = 1;

/// NewType to store a count and a bool together
/// Where the count is the playlist size estimate and the bool for whether it has already been set to a non-default
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountStore(usize, bool);

impl CountStore {
	/// Get wheter a count set (non-default) has occured
	pub fn has_been_set(&self) -> bool {
		return self.1;
	}
}

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
	count_estimate:          Cell<CountStore>,

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
		extra_ytdl_args: &[String],
	) -> Self {
		// process extra arguments into separated arguments of key and value (split once)
		let extra_cmd_args = extra_ytdl_args
			.iter()
			.flat_map(|v| {
				if let Some((split1, split2)) = v.split_once(' ') {
					return Vec::from([OsString::from(split1), OsString::from(split2)]);
				}
				return Vec::from([OsString::from(v)]);
			})
			.collect();

		return Self {
			audio_only_enable,
			extra_command_arguments: extra_cmd_args,
			print_stdout_debug,
			count_estimate: Cell::new(CountStore(DEFAULT_COUNT_ESTIMATE, false)),
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
			self.count_estimate.replace(CountStore(DEFAULT_COUNT_ESTIMATE, true));
		} else {
			self.count_estimate.replace(CountStore(count, true));
		}
	}

	/// Get the a copy of the current [CountStore]
	pub fn get_count_store(&self) -> CountStore {
		return self.count_estimate.get();
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
		if self.current_url.is_empty() {
			panic!("Expected \"current_url\" to not be empty at this point");
		}

		return &self.current_url;
	}

	fn print_command_stdout(&self) -> bool {
		return self.print_stdout_debug;
	}

	fn get_count_estimate(&self) -> usize {
		return self.count_estimate.get().0;
	}

	fn sub_langs(&self) -> Option<&String> {
		return self.sub_langs;
	}
}
