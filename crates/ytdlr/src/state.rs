//! Module for State Struct for all commands

use std::path::{
	Path,
	PathBuf,
};

use libytdlr::{
	data::cache::media_info::MediaInfo,
	traits::context::DownloadOptions,
};

/// Struct to keep configuration data for the [`DownloadOptions`] trait
#[derive(Debug, PartialEq, Clone)]
pub struct DownloadState {
	/// Enable downloading / converting to audio only format
	audio_only_enable:       bool,
	/// Extra arguments to pass to ytdl
	extra_command_arguments: Vec<std::ffi::OsString>,
	/// Print youtube-dl stdout as trace logs
	print_stdout_debug:      bool,
	/// The Path to download to
	download_path:           PathBuf,
	/// A Helper to generate the archive and allocation hints
	count_result:            Vec<MediaInfo>,

	/// Force implementation of [`DownloadOptions::gen_archive`] to only output the latest 500 sqlite inserted media elements to the youtube-dl archive
	force_genarchive_bydate: bool,
	/// Force implementation of [`DownloadOptions::gen_archive`] to entirely dump all records in sqlite to the youtube-dl archive
	force_genarchive_all:    bool,
	/// Force to not use a yt-dl archive, but still save to ytdlr archive
	force_no_archive:        bool,

	/// Set the current URL to be downloaded
	current_url: String,
}

impl DownloadState {
	/// Create a new instance of [`DownloadState`] with the required options
	pub fn new(
		audio_only_enable: bool,
		print_stdout_debug: bool,
		download_path: PathBuf,
		force_genarchive_bydate: bool,
		force_genarchive_all: bool,
		force_no_archive: bool,
	) -> Self {
		return Self {
			audio_only_enable,
			// for now, there are no extra arguments supported
			extra_command_arguments: Vec::default(),
			print_stdout_debug,
			count_result: Vec::default(),
			download_path,

			force_genarchive_bydate,
			force_genarchive_all,
			force_no_archive,

			current_url: String::default(),
		};
	}

	/// Set the current url ot be downloaded
	pub fn set_current_url<S: AsRef<str>>(&mut self, new_url: S) {
		// replace the already allocated string with the "new_url" without creating a new string
		self.current_url.replace_range(.., new_url.as_ref());
	}

	/// Set "count_result" for generating the archive and for "get_count_estimate"
	pub fn set_count_result(&mut self, new_vec: Vec<MediaInfo>) {
		self.count_result = new_vec;
	}

	/// Get the "download_path" stored in the instance as reference
	pub fn get_download_path(&self) -> &Path {
		return &self.download_path;
	}
}

impl DownloadOptions for DownloadState {
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

		if self.force_no_archive {
			debug!("force_no_archive, not outputting any ytdl archive");

			return Some(Box::new([].into_iter()));
		}

		// function to use to format all output to a youtube-dl archive, consistent across all options
		let fmfn = |v: Result<libytdlr::data::sql_models::Media, diesel::result::Error>| {
			let v = v.ok()?;
			return Some(format!("{} {}\n", v.provider, v.media_id));
		};

		if self.force_genarchive_all {
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

		if !self.count_result.is_empty() && !self.force_genarchive_bydate {
			// case where "count_result" is available
			let lines_iter = media_archive::dsl::media_archive
				.filter(
					media_archive::media_id.eq_any(
						self.count_result
							.iter()
							.map(|v| return v.id.as_str())
							.collect::<Vec<&str>>(),
					),
				)
				// only filter based on the id, not also provider
				// this may be slightly inaccurate, but it is better than a way more heavy query
				// .filter(
				// 	media_archive::provider.eq_any(
				// 		self.count_result
				// 			.iter()
				// 			.map(|v| {
				// 				return v
				// 					.provider
				// 					.as_ref()
				// 					.map_or_else(|| return "unknown (none-provided)", |v| return v.to_str());
				// 			})
				// 			.collect::<Vec<&str>>(),
				// 	),
				// )
				// the following is some black-magic that rust-analyzer does not understand (no useful intellisense available)
				.load_iter::<Media, diesel::connection::DefaultLoadingMode>(connection)
				.ok()?
				// the following has some explicit type-annotation for the argument, because otherwise rust-analyzer does not provide any types
				.filter_map(fmfn);

			return Some(Box::new(lines_iter));
		}

		// fallback case where "count_result" is not available
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
		// set "4" as default, so that even in a single download it is already allocated
		// and in case it is a playlist to have a small buffer
		let len = self.count_result.len();
		if len < 4 {
			return 4;
		}

		return len;
	}
}
