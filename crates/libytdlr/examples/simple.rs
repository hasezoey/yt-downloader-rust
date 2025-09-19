use std::path::Path;

use libytdlr::{
	chrono::NaiveDate,
	main::download::{
		DownloadOptions,
		DownloadProgress,
		MINIMAL_YTDL_VERSION,
		download_single,
	},
	spawn::ytdl::{
		require_ytdl_installed,
		ytdl_parse_version_naivedate,
	},
};

struct Options {
	ytdl_version: NaiveDate,
	url:          String,
	// ... fields corresponding to the trait impl below
}

impl DownloadOptions for Options {
	fn audio_only(&self) -> bool {
		return false;
	}

	fn extra_ytdl_arguments(&self) -> Vec<&std::ffi::OsStr> {
		return Vec::new();
	}

	fn download_path(&self) -> &std::path::Path {
		return Path::new("/tmp/download");
	}

	fn gen_archive<'a>(
		&'a self,
		_connection: &'a mut diesel::SqliteConnection,
	) -> Option<Box<dyn Iterator<Item = String> + 'a>> {
		return None;
	}

	fn get_url(&self) -> &str {
		return &self.url;
	}

	fn print_command_log(&self) -> bool {
		return false;
	}

	fn save_command_log(&self) -> bool {
		return false;
	}

	fn sub_langs(&self) -> Option<&str> {
		return None;
	}

	fn ytdl_version(&self) -> chrono::NaiveDate {
		return self.ytdl_version;
	}

	fn get_audio_format(&self) -> libytdlr::main::download::FormatArgument<'_> {
		return "best";
	}

	fn get_video_format(&self) -> libytdlr::main::download::FormatArgument<'_> {
		return "mkv";
	}
}

fn progress_callback(event: DownloadProgress) {
	match event {
		DownloadProgress::UrlStarting => println!("Starting URL"),
		DownloadProgress::Skipped(_, skipped_type) => println!("Skipped because: {skipped_type:#?}"),
		DownloadProgress::SingleStarting(id, title) => println!("Starting \"{id}\": {title}"),
		DownloadProgress::SingleProgress(id, progress) => {
			let id = id.unwrap_or("<unknown>".into());
			println!("Progress for \"{id}\" {progress}");
		},
		DownloadProgress::SingleFinished(id) => println!("Finished \"{id}\""),
		DownloadProgress::UrlFinished(count) => println!("Finished URL; Downloaded {count} media"),
		DownloadProgress::PlaylistInfo(count) => println!("Found playlist with {count} media"),
	}
}

fn main() -> Result<(), libytdlr::Error> {
	let ytdl_version = require_ytdl_installed()?;

	let ytdl_version = ytdl_parse_version_naivedate(&ytdl_version).unwrap_or_else(|_| {
		eprintln!("Could not determine youtube-dl version properly, using default");

		return MINIMAL_YTDL_VERSION;
	});

	let mut args = std::env::args();

	let _ = args.next();

	let url = args.next().expect("Expected a URL as a argument");

	assert!(!url.is_empty(), "Given URL is empty!");

	let connection = None;
	let options = Options { ytdl_version, url };

	let mut result_vec = Vec::new();

	download_single(connection, &options, progress_callback, &mut result_vec)?;

	println!("Finished downloading everything, all media: {result_vec:#?}");

	return Ok(());
}
