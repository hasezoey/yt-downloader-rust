use crate::{
	clap_conf::*,
	commands::download::quirks::apply_metadata,
	state::DownloadState,
	utils,
};
use colored::{
	Color,
	Colorize,
};
use diesel::SqliteConnection;
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
	ProgressStyle,
};
use libytdlr::{
	data::cache::{
		media_info::MediaInfo,
		media_provider::MediaProvider,
	},
	main::download::YTDL_ARCHIVE_PREFIX,
	traits::download_options::DownloadOptions,
	*,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
	cell::RefCell,
	collections::HashMap,
	io::{
		BufRead,
		BufReader,
		BufWriter,
		Error as ioError,
		Write,
	},
	path::{
		Path,
		PathBuf,
	},
};
use sysinfo::SystemExt;

/// Static for easily referencing the 100% length for a progressbar
const PG_PERCENT_100: u64 = 100;
/// Static size the Download Progress Style will take (plus some spacers)
/// currently accounts for `[00/??] [00:00:00] ### `
const STYLE_STATIC_SIZE: usize = 23;

struct Recovery {
	/// The path where the recovery file will be at
	pub path: PathBuf,
	/// The Writer to the file, open while this struct is not dropped
	writer:   Option<BufWriter<std::fs::File>>,
}

/// Helper to quickly check for termination
fn check_termination() -> Result<(), crate::Error> {
	// handle terminate
	if crate::TERMINATE
		.read()
		.map_err(|err| return crate::Error::other(format!("{err}")))?
		.should_terminate()
	{
		return Err(crate::Error::other("Termination Requested"));
	}

	return Ok(());
}

impl Recovery {
	/// Recovery file prefix
	const RECOVERY_PREFIX: &str = "recovery_";

	/// Create a new instance, without opening a file
	pub fn new<P>(path: P) -> std::io::Result<Self>
	where
		P: AsRef<Path>,
	{
		let path: PathBuf = libytdlr::utils::to_absolute(path)?; // absolutize the path so that "parent" does not return empty
		Self::check_path(&path)?; // check that the path is valid, and not only when trying to open it (when it would already be too late)
		return Ok(Self { path, writer: None });
	}

	/// Check a given path if it is valid to be wrote in
	fn check_path(path: &Path) -> std::io::Result<()> {
		// check that the given path does not already exist, as to not overwrite it
		if path.exists() {
			return Err(std::io::Error::new(
				std::io::ErrorKind::AlreadyExists,
				"Recovery File Path already exists!",
			));
		}
		// check that the given path has a parent
		let parent = path.parent().ok_or_else(|| {
			return std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"Failed to get the parent for the Recovery File!",
			);
		})?;
		// check that the parent already exists
		if !parent.exists() {
			return Err(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"Recovery File directory does not exist!",
			));
		}

		// check that the parent is writeable
		let meta = std::fs::metadata(parent)?;

		if meta.permissions().readonly() {
			return Err(std::io::Error::new(
				std::io::ErrorKind::PermissionDenied,
				"Recovery File directory is not writeable!",
			));
		}

		return Ok(());
	}

	/// Get the current "self.writer" or open a new one if not existing
	fn get_writer_or_open(&mut self) -> std::io::Result<&mut BufWriter<std::fs::File>> {
		if self.writer.is_none() {
			self.open_writer()?;
		}

		// "unwrap" because we can safely assume that "self.writer" is "Some" here
		return Ok(self.writer.as_mut().unwrap());
	}

	/// Open a new writer and place it into [`Self::writer`]
	fn open_writer(&mut self) -> std::io::Result<()> {
		let writer = BufWriter::new(std::fs::File::create(&self.path)?);
		self.writer.replace(writer);

		return Ok(());
	}

	/// Write the given MediaInfo-Vec to the file
	/// will not do anything if `media_arr` is empty
	pub fn write_recovery(&mut self, media_arr: &MediaInfoArr) -> std::io::Result<()> {
		// dont write a empty recovery file
		if media_arr.is_empty() {
			debug!("Nothing to write, not creating a recovery");
			return Ok(());
		}

		let writer = self.get_writer_or_open()?;
		// save the entries sorted
		let media_sorted_vec = media_arr.as_sorted_vec();
		for media_helper in media_sorted_vec {
			writer.write_all(Self::fmt_line(&media_helper.data).as_bytes())?;
		}

		return Ok(());
	}

	/// Format the input "media" to a recovery file line
	#[inline]
	pub fn fmt_line(media: &data::cache::media_info::MediaInfo) -> String {
		return format!(
			"'{}'-'{}'-{}\n",
			media
				.provider
				.as_ref()
				.expect("Expected downloaded media to have a provider"),
			media.id,
			media.title.as_ref().expect("Expected downloaded media to have a title")
		);
	}

	/// Try to create a MediaInfo from a given line
	pub fn try_from_line(line: &str) -> Option<data::cache::media_info::MediaInfo> {
		/// Regex for getting the provider,id,title from a line in a recovery format
		/// cap1: provider, cap2: id, cap3: title
		static FROM_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^'([^']+)'-'([^']+)'-(.+)$").unwrap();
		});

		let cap = FROM_LINE_REGEX.captures(line)?;

		return Some(
			data::cache::media_info::MediaInfo::new(&cap[2])
				.with_provider(data::cache::media_provider::MediaProvider::from_str_like(&cap[1]))
				.with_title(&cap[3]),
		);
	}

	/// Try to read the recovery from the given path
	pub fn read_recovery(path: &Path) -> Result<impl Iterator<Item = MediaInfo>, crate::Error> {
		if !path.exists() {
			return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Recovery File Path does not exist").into());
		}
		// error in case of not being a file, maybe consider changeing this to a function and ignoring if not existing
		if !path.is_file() {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, "Recovery File Path is not a file").into());
		}
		let file_handle = BufReader::new(std::fs::File::open(path)?);

		let iter = file_handle
			.lines()
			.filter_map(|v| return v.ok())
			.filter_map(|v| return Self::try_from_line(&v));

		return Ok(iter);
	}

	/// Clean-up the current instance, if it has written anything
	pub fn finish(mut self) {
		// only remove file if there is a writer, because only then the file exists
		if self.writer.is_some() {
			let path = self.path;

			self.writer.take(); // drop "writer" to explicitly close the file handle

			Self::remove_file(&path);
		}
	}

	/// Tries to remove the given file, ignoring if the file does not exist and otherwise just logging the error
	pub fn remove_file(path: &Path) {
		std::fs::remove_file(path).unwrap_or_else(|err| match err.kind() {
			std::io::ErrorKind::NotFound => (),
			_ => info!("Error removing recovery file. Error: {}", err),
		});
	}
}

/// Helper struct to keep the order of download / addition and the data, with names
struct MediaHelper {
	/// The actual [`MediaInfo`] that is stored
	data:    MediaInfo,
	/// The order of which it was added / downloaded in (used for editing loop)
	order:   usize,
	/// Extra Comment if necessary
	comment: Option<String>,
}

impl MediaHelper {
	pub fn new(data: MediaInfo, order: usize, comment: Option<String>) -> Self {
		return Self { data, order, comment };
	}
}

/// Custom HashMap for [`MediaInfo`] to keep usage easy
struct MediaInfoArr {
	mediainfo_map: HashMap<String, MediaHelper>,
	next_order:    usize,
}

impl MediaInfoArr {
	/// Create a new empty instance
	pub fn new() -> Self {
		return Self {
			mediainfo_map: HashMap::default(),
			next_order:    0,
		};
	}

	/// Check if the mediainfo_map is empty, see [`HashMap::is_empty`]
	pub fn is_empty(&self) -> bool {
		return self.mediainfo_map.is_empty();
	}

	/// Insert a [`MediaInfo`] into the map, updating the old value if existed and returing the old value
	pub fn insert(&mut self, mediainfo: MediaInfo) -> Option<MediaHelper> {
		return self._insert(mediainfo, None);
	}
	/// Insert a [`MediaInfo`] into the map, updating the old value if existed and returing the old value
	/// with a comment
	pub fn insert_with_comment<C>(&mut self, mediainfo: MediaInfo, comment: C) -> Option<MediaHelper>
	where
		C: Into<String>,
	{
		return self._insert(mediainfo, Some(comment.into()));
	}

	/// Helper for [`Self::insert`] and [`Self::insert_with_comment`] to only have one implementation
	fn _insert(&mut self, mediainfo: MediaInfo, comment: Option<String>) -> Option<MediaHelper> {
		let order = self.next_order;
		self.next_order += 1;

		let key = format!(
			"{}-{}",
			mediainfo
				.provider
				.as_ref()
				.map_or_else(|| return "unknown", |v| return v.to_str()),
			mediainfo.id,
		);

		return self
			.mediainfo_map
			.insert(key, MediaHelper::new(mediainfo, order, comment));
	}

	/// Get a value inside the HashMap mutably
	pub fn get_mut<K>(&mut self, key: K) -> Option<&mut MediaHelper>
	where
		K: AsRef<str>,
	{
		return self.mediainfo_map.get_mut(key.as_ref());
	}

	/// Directly pass through `additional` to [`HashMap::reserve`]
	pub fn reserve(&mut self, additional: usize) {
		self.mediainfo_map.reserve(additional);
	}

	/// Get a sorted [`Vec`] from the current HashMap
	/// only contains references to what is in the HashMap, not moving the values
	pub fn as_sorted_vec(&self) -> Vec<&MediaHelper> {
		let mut vec: Vec<&MediaHelper> = self.mediainfo_map.values().collect();

		vec.sort_by(|a, b| return a.order.cmp(&b.order));

		return vec;
	}
}

/// Truncate the given message to a lower size so that the progressbar does not do new-lines
/// truncation is required because indicatif would do new-lines, and adding truncation would only work with a (static) maximum size
/// NOTE: this currently only gets run once for each "SingleStartin" instead of every tick, so resizing the truncate will not be done (until next media)
fn truncate_message_term_width<M>(msg: &M) -> String
where
	M: AsRef<str>,
{
	let display_width_available = term_size::dimensions().map(|(w, _h)| {
		return w.saturating_sub(STYLE_STATIC_SIZE);
	});

	let display_width_available = match display_width_available {
		Some(v) => v,
		None => return msg.as_ref().into(),
	};

	return utils::truncate_message_display_pos(msg, display_width_available, true).to_string();
}

/**
 * Find all files that match the temporary ytdl archive name, and remove all whose pid is not alive anymore
 */
fn find_and_remove_tmp_archive_files(path: &Path) -> Result<(), ioError> {
	if !path.is_dir() {
		return Err(ioError::new(
			std::io::ErrorKind::Other, // TODO: replace "Other" with "NotADirectory" when stable
			"Path to find recovery files is not existing or a directory!",
		));
	}

	// IMPORTANT: currently sysinfo creates threads, but never closes them (even when going out of scope)
	// see https://github.com/GuillaumeGomez/sysinfo/issues/927
	let mut s = sysinfo::System::new();
	s.refresh_processes();

	for file in path.read_dir()?.filter_map(|res| {
		let entry = res.ok()?;

		let path = entry.path();
		let file_name = path.file_name()?;
		if path.is_file() && file_name.to_string_lossy().starts_with(YTDL_ARCHIVE_PREFIX) {
			return Some(path);
		}
		return None;
	}) {
		let file_name = file.file_name().unwrap().to_string_lossy(); // unwrap because non-file_name containing paths should be sorted out in the "filter_map"
		info!("Trying to match tmp yt-dl archive file: \"{}\"", file_name);
		let pid_str = {
			/// Regex for extracting the pid from the filename
			/// cap1: pid str
			static PID_OF_ARCHIVE: Lazy<Regex> = Lazy::new(|| {
				return Regex::new(r"(?m)^ytdl_archive_(\d+)\.txt$").unwrap();
			});

			let cap = PID_OF_ARCHIVE.captures(&file_name);

			let cap = if let Some(cap) = cap {
				cap
			} else {
				continue;
			};

			cap.get(1).expect("Expected group 1 to always exist").as_str()
		};
		let pid_of_file = {
			let res = pid_str.parse::<usize>();
			if res.is_err() {
				continue;
			}
			res.unwrap() // unwrap because "Err" is checked above
		};
		// check that the pid of the file is actually not running anymore
		// and just ignore them if the process exists
		if s.process(sysinfo::Pid::from(pid_of_file)).is_some() {
			info!("Found tmp yt-dl archive file for pid {pid_of_file}, but the process still existed");
			continue;
		}
		std::fs::remove_file(file).unwrap_or_else(|err| match err.kind() {
			std::io::ErrorKind::NotFound => (),
			_ => info!("Error removing found tmp yt-dl archvie file. Error: {}", err),
		});
	}

	return Ok(());
}

/// Handler function for the "download" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), crate::Error> {
	utils::require_ytdl_installed()?;

	let only_recovery = sub_args.urls.is_empty();

	if only_recovery {
		if sub_args.no_check_recovery {
			return Err(crate::Error::other("At least one URL is required"));
		}

		println!(
			"{} No URLs were provided, only checking recovery! To disable allowing 0 URLs, use \"--no-check-recovery\"",
			"WARN".color(Color::TrueColor { r: 255, g: 135, b: 0 })
		)
	}

	/// ProgressBar Style for download, will look like "[0/0] [00:00:00] [#>-] CustomMsg"
	static DOWNLOAD_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
		return ProgressStyle::default_bar()
			.template("{prefix:.dim} [{elapsed_precise}] {wide_bar:.cyan/blue} {msg}")
			.expect("Expected ProgressStyle template to be valid")
			.progress_chars("#>-");
	});

	let tmp_path = main_args
		.tmp_path
		.as_ref()
		.map_or_else(|| return std::env::temp_dir(), |v| return v.clone())
		.join("ytdl_rust_tmp");

	std::fs::create_dir_all(&tmp_path)?;

	let pgbar: ProgressBar = ProgressBar::new(PG_PERCENT_100).with_style(DOWNLOAD_STYLE.clone());
	utils::set_progressbar(&pgbar, main_args);

	let mut download_state = DownloadState::new(
		sub_args.audio_only_enable,
		sub_args.print_youtubedl_stdout,
		tmp_path,
		sub_args.archive_mode,
		sub_args.sub_langs.as_ref(),
	);

	// already create the vec for finished media, so that the finished ones can be stored in case of error
	let mut finished_media = MediaInfoArr::new();
	let mut recovery = Recovery::new(download_state.get_download_path().join(format!(
		"{}{}",
		Recovery::RECOVERY_PREFIX,
		std::process::id()
	)))?;

	// recover files that are not in a recovery but are still considered editable
	// only do this in "only_recovery" mode (no urls) to not accidentally use from other processes
	if only_recovery {
		for media in utils::find_editable_files(download_state.get_download_path())? {
			finished_media.insert_with_comment(media, "Found Editable File");
		}
	}

	find_and_remove_tmp_archive_files(download_state.get_download_path())?;

	// run AFTER finding all files, so that the correct filename is already set for files, and only information gets updated
	let found_recovery_files =
		try_find_and_read_recovery_files(&mut finished_media, download_state.get_download_path())?;

	crate::TERMINATE
		.write()
		.map_err(|err| return crate::Error::other(format!("{err}")))?
		.set_msg(String::from(
			"Termination has been requested, press again to terminate immediately",
		));

	// TODO: consider cross-checking archive if the files from recovery are already in the archive and get a proper title

	match download_wrapper(
		main_args,
		sub_args,
		&pgbar,
		&mut download_state,
		&mut finished_media,
		only_recovery,
	) {
		Ok(_) => (),
		Err(err) => {
			let res = recovery.write_recovery(&finished_media);

			// log recovery write error, but do not modify original error
			if let Err(rerr) = res {
				warn!("Failed to write recovery: {}", rerr)
			}

			return Err(err.into());
		},
	}

	// do some cleanup
	// remove the recovery file, because of a successfull finish
	recovery.finish();

	// clean-up the recovery files that were read earlier
	for file in found_recovery_files {
		Recovery::remove_file(&file);
	}

	return Ok(());
}

/// Wrapper for [`command_download`] to house the part where in case of error a recovery needs to be written
fn download_wrapper(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	pgbar: &ProgressBar,
	download_state: &mut DownloadState,
	finished_media: &mut MediaInfoArr,
	only_recovery: bool,
) -> Result<(), ioError> {
	if !only_recovery {
		do_download(main_args, sub_args, pgbar, download_state, finished_media)?;
	} else {
		info!("Skipping download because of \"only_recovery\"");
	}

	let download_path = download_state.get_download_path();

	// TODO: check if the following is still something that should be done

	// merge found filenames into existing mediainfo
	for new_media in utils::find_editable_files(download_path)? {
		if let Some(media) = finished_media.get_mut(&format!(
			"{}-{}",
			new_media
				.provider
				.as_ref()
				.map_or_else(|| return "unknown", |v| return v.to_str()),
			new_media.id
		)) {
			let new_media_filename = new_media
				.filename
				.expect("Expected MediaInfo to have a filename from \"try_from_filename\"");

			media.data.set_filename(new_media_filename);
		}
	}

	edit_media(main_args, sub_args, download_path, finished_media)?;

	finish_media(main_args, sub_args, download_path, pgbar, finished_media)?;

	return Ok(());
}

/// Characters to use if a state for the ProgressBar is unknown
const PREFIX_UNKNOWN: &str = "??";

/// Helper function to consistently set the progressbar prefix
fn set_progressbar_prefix(
	pgbar: &ProgressBar,
	download_info: std::cell::Ref<DownloadInfo>,
	download_state: &DownloadState,
	unknown_playlist_count: bool,
	unknown_current_count: bool,
) {
	let current_count = if unknown_current_count {
		PREFIX_UNKNOWN.into()
	} else {
		download_info.playlist_count.to_string()
	};
	let playlist_count = if unknown_playlist_count {
		PREFIX_UNKNOWN.into()
	} else {
		download_state.get_count_estimate().to_string()
	};
	pgbar.set_prefix(format!("[{}/{}]", current_count, playlist_count));
}

/// Helper struct to keep track of some state, while having named fields instead of numbered tuple fields
#[derive(Debug, PartialEq, Clone)]
struct DownloadInfo {
	/// Count of how many Media have been downloaded in the current URL (playlist)
	/// because it does not include media already in archive
	pub playlist_count: usize,
	/// Media id of the current Media being downloaded
	pub id:             String,
	/// Title of the current Media being downloaded
	pub title:          String,
	/// Index of the current url being processed
	/// not 0 based
	pub url_index:      usize,
}

impl DownloadInfo {
	/// Create a new instance of [Self] with all the provided options
	pub fn new(playlist_count: usize, id: String, title: String, url_index: usize) -> Self {
		return Self {
			playlist_count,
			id,
			title,
			url_index,
		};
	}

	/// Create a new default instance of [Self], but with a set "url_index"
	pub fn new_with_url_index(url_index: usize) -> Self {
		return Self::new(0, Default::default(), Default::default(), url_index);
	}
}

impl Default for DownloadInfo {
	fn default() -> Self {
		return Self::new(0, Default::default(), Default::default(), 0);
	}
}

/// Do the download for all provided URL's
fn do_download(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	pgbar: &ProgressBar,
	download_state: &mut DownloadState,
	finished_media: &mut MediaInfoArr,
) -> Result<(), crate::Error> {
	let mut maybe_connection: Option<SqliteConnection> = {
		if let Some(ap) = main_args.archive_path.as_ref() {
			Some(utils::handle_connect(ap, pgbar, main_args)?.1)
		} else {
			None
		}
	};

	// store "download_state" in a refcell, because rust complains that a borrow is made in "download_pgcb" and also later used while still in scope
	let download_state_cell: RefCell<&mut DownloadState> = RefCell::new(download_state);
	let download_info: RefCell<DownloadInfo> = RefCell::new(DownloadInfo::default());
	let url_len = sub_args.urls.len();
	set_progressbar_prefix(pgbar, download_info.borrow(), *download_state_cell.borrow(), true, true);
	// track total count finished (no error)
	let total_count = std::sync::atomic::AtomicUsize::new(0);
	let download_pgcb = |dpg| match dpg {
		main::download::DownloadProgress::AllStarting => {
			pgbar.reset();
			pgbar.set_message(""); // ensure it is not still present across finish and reset
			let url_index = download_info.borrow().url_index;
			download_info.replace(DownloadInfo::new_with_url_index(url_index));
		},
		main::download::DownloadProgress::SingleStarting(id, title) => {
			let new_count = download_info.borrow().playlist_count + 1;
			let url_index = download_info.borrow().url_index;
			download_info.replace(DownloadInfo::new(new_count, id, title, url_index));

			pgbar.reset();
			pgbar.set_length(PG_PERCENT_100); // reset length, because it may get changed because of connection insert
			let download_info_borrowed = download_info.borrow();
			set_progressbar_prefix(
				pgbar,
				download_info.borrow(),
				*download_state_cell.borrow(),
				false,
				false,
			);
			pgbar.set_message(truncate_message_term_width(&download_info_borrowed.title));
			pgbar.println(format!("Downloading: {}", download_info_borrowed.title));
		},
		main::download::DownloadProgress::SingleProgress(_maybe_id, percent) => {
			pgbar.set_position(percent.into());
		},
		main::download::DownloadProgress::SingleFinished(_id) => {
			pgbar.finish_and_clear();
			pgbar.println(format!("Finished Downloading: {}", download_info.borrow().title));
			// pgbar.finish_with_message();
		},
		main::download::DownloadProgress::AllFinished(new_count) => {
			pgbar.finish_and_clear();
			let total = total_count.fetch_add(new_count, std::sync::atomic::Ordering::AcqRel) + new_count;
			// print how many media has been downloaded since last "AllStarting" and how many in total in this run
			pgbar.println(format!(
				"Finished Downloading {new_count} new Media (For a total of {total} Media) (url {}/{})",
				download_info.borrow().url_index,
				url_len
			));
		},
		main::download::DownloadProgress::PlaylistInfo(new_count) => {
			download_state_cell.borrow().set_count_estimate(new_count);
		},
	};

	for (index, url) in sub_args.urls.iter().enumerate() {
		// handle terminate
		check_termination()?;

		// index plus one, to match .len, to not have 0-index for display
		let index_p = index + 1;

		download_info.borrow_mut().url_index = index_p;

		println!("Starting download of \"{}\" ({}/{})", url, index_p, url_len);

		download_state_cell.borrow_mut().set_current_url(url);

		let new_media = libytdlr::main::download::download_single(
			maybe_connection.as_mut(),
			*download_state_cell.borrow(),
			download_pgcb,
		)?;

		if let Some(ref mut connection) = maybe_connection {
			pgbar.reset();
			pgbar.set_length(new_media.len().try_into().expect("Failed to convert usize to u64"));
			for media in new_media.iter() {
				pgbar.inc(1);
				libytdlr::main::archive::import::insert_insmedia(&media.into(), connection)?;
			}
			pgbar.finish_and_clear();
		}

		// quick hint so that insertion is faster
		// because insertion is one element at a time
		finished_media.reserve(new_media.len());

		for media in new_media {
			finished_media.insert(media);
		}
	}

	// remove ytdl_archive_pid.txt file again, because otherwise over many usages it can become bloated
	std::fs::remove_file(libytdlr::main::download::get_archive_name(
		download_state_cell.borrow().download_path(),
	))
	.unwrap_or_else(|err| {
		info!("Removing ytdl archive failed. Error: {}", err);
		return;
	});

	return Ok(());
}

/// Start editing loop for all provided media
fn edit_media(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	final_media: &MediaInfoArr,
) -> Result<(), crate::Error> {
	if !main_args.is_interactive() {
		info!("Skipping asking for media, because \"is_interactive\" is \"false\"");
		return Ok(());
	}

	let media_sorted_vec = final_media.as_sorted_vec();
	// ask for editing
	// TODO: consider renaming before asking for edit
	'for_media_loop: for media_helper in media_sorted_vec.iter() {
		// handle terminate
		check_termination()?;

		let media = &media_helper.data;
		let media_filename = match &media.filename {
			Some(v) => v,
			None => {
				println!("\"{}\" did not have a filename!", media.id);
				println!("debug: {media:#?}");
				continue 'for_media_loop;
			},
		};
		let media_path = download_path.join(media_filename);
		// extra loop is required for printing the help and asking again
		'ask_do_loop: loop {
			let input = utils::get_input(
				&format!(
					"Edit Media \"{}\"?{}",
					media
						.title
						.as_ref()
						.expect("Expected MediaInfo to have a title from \"try_from_filename\""),
					media_helper
						.comment
						.as_ref()
						.map_or("".into(), |msg| format!(" ({msg})"))
				),
				&["h", "y", "N", "a", "v", "p"],
				"n",
			)?;

			match input.as_str() {
				"n" => continue 'for_media_loop,
				"y" => match utils::get_filetype(media_filename) {
					utils::FileType::Video => {
						println!("Found filetype to be of video");
						run_editor_wrap(&sub_args.video_editor, &media_path)?
					},
					utils::FileType::Audio => {
						println!("Found filetype to be of audio");
						run_editor_wrap(&sub_args.audio_editor, &media_path)?
					},
					utils::FileType::Unknown => {
						// if not FileType could be found, ask user what to do
						match utils::get_input(
							"Could not find suitable editor for extension, [a]udio editor, [v]ideo editor, a[b]ort, [n]ext.",
							&["a", "v", "b", "n"],
							"",
						)?
						.as_str()
						{
							"a" => run_editor_wrap(&sub_args.audio_editor, &media_path)?,
							"v" => run_editor_wrap(&sub_args.video_editor, &media_path)?,
							"b" => return Err(crate::Error::other("Abort Selected")),
							"n" => continue 'for_media_loop,
							_ => unreachable!("get_input should only return a OK value from the possible array"),
						}
					},
				},
				"h" => {
					println!(
						"Help:\n\
					[h] print help (this)\n\
					[n] skip element and move onto the next one\n\
					[y] edit element, automatically choose editor\n\
					[a] edit element with audio editor\n\
					[v] edit element with video editor\
					"
					);
					continue 'ask_do_loop;
				},
				"a" => {
					run_editor_wrap(&sub_args.audio_editor, &media_path)?;
				},
				"v" => {
					run_editor_wrap(&sub_args.video_editor, &media_path)?;
				},
				"p" => {
					utils::run_editor(&sub_args.player_editor, &media_path)?;

					// re-do the loop, because it was only played
					continue 'ask_do_loop;
				},
				_ => unreachable!("get_input should only return a OK value from the possible array"),
			}

			// when getting here, the media needs to be re-thumbnailed
			debug!("Re-applying thumbnail for media");
			if let Some(image_path) = libytdlr::main::rethumbnail::find_image(&media_path)? {
				// re-apply thumbnail to "media_path", and have the output be the same path
				// "re_thumbnail_with_tmp" will handle that the original will only be overwritten once successfully finished
				libytdlr::main::rethumbnail::re_thumbnail_with_tmp(&media_path, image_path, &media_path)?;
			} else {
				warn!(
					"No Image found for media, not re-applying thumbnail! Media: \"{}\"",
					media
						.title
						.as_ref()
						.expect("Expected MediaInfo to have a title from \"try_from_filename\"")
				);
			}

			continue 'for_media_loop;
		}
	}

	return Ok(());
}

/// Wrap [utils::run_editor] calls to apply quirks in all cases - but only when editor is actually run
fn run_editor_wrap(maybe_editor: &Option<PathBuf>, file: &Path) -> Result<(), crate::Error> {
	// re-apply full metadata after a editor run, because currently audacity does not properly handle custom tags
	// see https://github.com/audacity/audacity/issues/3733
	let metadata_file = quirks::save_metadata(file)?;

	utils::run_editor(maybe_editor, file)?;

	// re-apply full metadata after a editor run, because currently audacity does not properly handle custom tags
	// see https://github.com/audacity/audacity/issues/3733
	if let Some(metadata_file) = metadata_file {
		apply_metadata(file, &metadata_file)?;

		match std::fs::remove_file(&metadata_file) {
			Ok(()) => (),
			Err(err) => {
				info!("Removing metadata file failed, error: {}", err);
			},
		};
	} else {
		debug!("No metadata file, not reapplying metadata");
	}

	return Ok(());
}

/// Module for keeping all quirk workaround functions and imports
mod quirks {
	use super::*;
	use libytdlr::spawn::ffmpeg::base_ffmpeg_hidebanner;
	use std::collections::HashSet;

	/// Save the Metadata of the given media file
	/// Returns the Path to the metadata file
	pub fn save_metadata<MF>(media_file: MF) -> Result<Option<PathBuf>, crate::Error>
	where
		MF: AsRef<Path>,
	{
		let media_file = media_file.as_ref();
		let metadata_file = {
			let mut tmp_metadata_file: PathBuf = media_file.to_path_buf();
			let mut file_name = tmp_metadata_file
				.file_name()
				.ok_or_else(|| {
					return crate::Error::other(format!(
						"Expected file to have a filename, File: \"{}\"",
						tmp_metadata_file.to_string_lossy()
					));
				})?
				.to_os_string();
			file_name.push(".metadata");
			tmp_metadata_file.set_file_name(file_name);

			tmp_metadata_file
		};

		info!("Saving Metadata of file \"{}\"", media_file.to_string_lossy());

		let metadata_format = get_metadata_type(media_file)?;

		let mut ffmpeg_cmd = base_ffmpeg_hidebanner(true); // overwrite metadata file if already exists

		ffmpeg_cmd.arg("-i");
		ffmpeg_cmd.arg(media_file);

		// nothing extra needs to be done for global, only stream needs stream selection
		if metadata_format == MetadataType::Stream {
			ffmpeg_cmd.args(["-map_metadata", "0:s:0"]);
		}

		ffmpeg_cmd.args(["-f", "ffmetadata"]);
		ffmpeg_cmd.arg(&metadata_file);

		debug!("Spawning ffmpeg to save metadata");

		let output = ffmpeg_cmd.output()?;

		let exit_status = output.status;

		if !exit_status.success() {
			debug!("ffmpeg did not exist successfully, displaying log:");
			debug!("STDERR {}", String::from_utf8_lossy(&output.stderr));

			return Err(crate::Error::other(format!(
				"ffmpeg metadata save command failed, code: {}",
				exit_status.code().map_or("None".into(), |v| return v.to_string())
			)));
		}

		if !metadata_file.exists() {
			warn!("metadata files does not exist after ffmpeg ran and exited successfully");
			return Ok(None);
		}

		return Ok(Some(metadata_file));
	}

	/// Extensions that store metadata in the global
	static GLOBAL_METADATA_EXT: Lazy<HashSet<&'static str>> = Lazy::new(|| {
		return HashSet::from(["mp3"]);
	});
	/// Extensions that store metadata in the stream
	static STREAM_METADATA_EXT: Lazy<HashSet<&'static str>> = Lazy::new(|| {
		return HashSet::from(["ogg"]);
	});

	fn get_format(media_file: &Path) -> Result<String, crate::Error> {
		trace!("Getting Format for file \"{}\"", media_file.to_string_lossy());

		let stdout = libytdlr::spawn::ffmpeg::ffmpeg_probe(media_file)?;
		let formats = libytdlr::spawn::ffmpeg::parse_format(&stdout)?.join(",");

		debug!("Found file to be of format \"{formats}\"");

		return Ok(formats);
	}

	fn get_metadata_type(media_file: &Path) -> Result<MetadataType, crate::Error> {
		let metadata_format = match get_format(media_file) {
			Ok(v) => v,
			Err(err) => {
				warn!("Spawning ffprobe to get the format for metadata failed, Error: {}", err);

				return ask_format(media_file);
			},
		};

		if GLOBAL_METADATA_EXT.contains(metadata_format.as_str()) {
			return Ok(MetadataType::Global);
		}

		if STREAM_METADATA_EXT.contains(metadata_format.as_str()) {
			return Ok(MetadataType::Stream);
		}

		warn!("Format \"{metadata_format}\" was not listed in the 2 HashSet's, manually asking for type");

		return ask_format(media_file);
	}

	#[derive(Debug, PartialEq)]
	enum MetadataType {
		Global,
		Stream,
	}

	/// Ask for manual metadata stream selection
	fn ask_format(input_file: &Path) -> Result<MetadataType, crate::Error> {
		// if not FileType could be found, ask user what to do
		return Ok(match utils::get_input(
			&format!("Could not determine which metadata type is used for file. Select manually: [g]lobal [s]tream\nFile: \"{}\"", input_file.to_string_lossy()),
			&["g", "s"],
			"",
		)?
		.as_str()
		{
			"g" => MetadataType::Global,
			"s" => MetadataType::Stream,
			_ => unreachable!("get_input should only return a OK value from the possible array"),
		});
	}

	/// Apply the given Metadata to the given media_file
	pub fn apply_metadata<MF, MD>(media_file: MF, metadata_file: MD) -> Result<(), crate::Error>
	where
		MF: AsRef<Path>,
		MD: AsRef<Path>,
	{
		let media_file = media_file.as_ref();
		let metadata_file = metadata_file.as_ref();

		let media_file_tmp = {
			let mut tmp_media_file_tmp = media_file.to_path_buf();
			let mut file_name = tmp_media_file_tmp
				.file_name()
				.ok_or_else(|| {
					return crate::Error::other(format!(
						"Expected file to have a filename, File: \"{}\"",
						tmp_media_file_tmp.to_string_lossy()
					));
				})?
				.to_os_string();
			file_name.push(".tmp");
			tmp_media_file_tmp.set_file_name(file_name);
			tmp_media_file_tmp
		};

		let mut ffmpeg_cmd = base_ffmpeg_hidebanner(true); // overwrite metadata file if already exists

		ffmpeg_cmd.arg("-i");
		ffmpeg_cmd.arg(media_file);

		ffmpeg_cmd.arg("-i");
		ffmpeg_cmd.arg(metadata_file);

		ffmpeg_cmd.args(["-map_metadata", "1", "-map_metadata:s:a", "1:g", "-codec", "copy"]);

		// explicitly setting output format, because ffmpeg tries to infer from output extension - which may fail
		match get_format(media_file) {
			Ok(media_file_format) => {
				ffmpeg_cmd.arg("-f");
				ffmpeg_cmd.arg(media_file_format);
			},
			Err(err) => {
				debug!("Getting format for input file failed, letting ffmpeg to automatically decide output format. Error: {}", err);
			},
		}

		ffmpeg_cmd.arg(&media_file_tmp);

		debug!("Spawning ffmpeg to apply metadata");

		let output = ffmpeg_cmd.output()?;

		let exit_status = output.status;

		if !exit_status.success() {
			debug!("ffmpeg did not exist successfully, displaying log:");
			debug!("STDERR {}", String::from_utf8_lossy(&output.stderr));

			return Err(crate::Error::other(format!(
				"ffmpeg metadata apply command failed, code: {}",
				exit_status.code().map_or("None".into(), |v| return v.to_string())
			)));
		}

		// rename can be used here, because both files exist in the same directory
		std::fs::rename(&media_file_tmp, media_file)?;

		return Ok(());
	}
}

/// Finish the given media by either opening up the tagger or moving to final destination
fn finish_media(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), ioError> {
	if final_media.mediainfo_map.is_empty() {
		println!("No files to move or tag");
		return Ok(());
	}

	// first set the draw-target so that any subsequent setting change does not cause a draw
	pgbar.set_draw_target(ProgressDrawTarget::hidden()); // so that it stays hidden until actually doing stuff
	pgbar.reset();
	pgbar.set_length(final_media.mediainfo_map.len().try_into().unwrap_or(u64::MAX));
	pgbar.set_message("Moving files");

	if main_args.is_interactive() && !sub_args.open_tagger {
		// the following is used to ask the user what to do with the media-files
		// current choices are:
		// move all media that is found to the final_directory (specified via options or defaulted), or
		// open the tagger and let the tagger handle the moving
		match utils::get_input("[m]ove Media to Output Directory or Open [p]icard?", &["m", "p"], "")?.as_str() {
			"m" => finish_with_move(sub_args, download_path, pgbar, final_media)?,
			"p" => finish_with_tagger(sub_args, download_path, pgbar, final_media)?,
			_ => unreachable!("get_input should only return a OK value from the possible array"),
		}
	} else {
		info!("non-interactive finish media, open_tagger: {}", sub_args.open_tagger);
		if sub_args.open_tagger {
			finish_with_tagger(sub_args, download_path, pgbar, final_media)?;
		} else {
			finish_with_move(sub_args, download_path, pgbar, final_media)?;
		}
	}

	// notify the user if there are still files that have not been moved
	if !utils::find_editable_files(download_path)?.is_empty() {
		println!("{} Found Editable file that have not been moved.\nConsider running recovery mode if no other ytdlr is running (with 0 URLs)", "WARN".color(Color::TrueColor { r: 255, g: 135, b: 0 }));
	}

	return Ok(());
}

/// Move all media in `final_media` to it final resting place in `download_path`
/// Helper to separate out the possible paths
fn finish_with_move(
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), ioError> {
	debug!("Moving all files to the final destination");

	let final_dir_path = sub_args.output_path.as_ref().map_or_else(
		|| {
			return dirs_next::download_dir()
				.unwrap_or_else(|| return PathBuf::from("."))
				.join("ytdlr-out");
		},
		|v| return v.clone(),
	);
	std::fs::create_dir_all(&final_dir_path)?;

	let mut moved_count = 0usize;
	pgbar.set_draw_target(ProgressDrawTarget::stderr());

	for media_helper in /* utils::find_editable_files(download_path)? */ final_media.mediainfo_map.values() {
		pgbar.inc(1);
		let media = &media_helper.data;
		let (media_filename, final_filename) = match utils::convert_mediainfo_to_filename(media) {
			Some(v) => v,
			None => {
				warn!("Found MediaInfo which returned \"None\" from \"convert_mediainfo_to_filename\", skipping (id: \"{}\")", media.id);

				continue;
			},
		};
		let from_path = download_path.join(media_filename);
		let to_path = final_dir_path.join(final_filename);
		trace!(
			"Copying file \"{}\" to \"{}\"",
			from_path.to_string_lossy(),
			to_path.to_string_lossy()
		);
		// copy has to be used, because it cannot be ensured the "final_path" is on the same file-system
		// and a "move"(mv) function does not exist in standard rust
		match std::fs::copy(&from_path, to_path) {
			Ok(_) => (),
			Err(err) => {
				println!("Couldnt move file \"{}\", error: {}", from_path.to_string_lossy(), err);
				continue;
			},
		};

		trace!("Removing file \"{}\"", from_path.to_string_lossy());
		// remove the original file, because copy was used
		std::fs::remove_file(from_path)?;

		moved_count += 1;
	}

	pgbar.finish_and_clear();

	println!(
		"Moved {} media files to \"{}\"",
		moved_count,
		final_dir_path.to_string_lossy()
	);

	return Ok(());
}

/// Move all media in `final_media` to a temporary `final` directory (still in the tmpdir) and open the tagger
/// Helper to separate out the possible paths
fn finish_with_tagger(
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), ioError> {
	debug!("Renaming files for Tagger");

	let final_dir_path = download_path.join("final");
	std::fs::create_dir_all(&final_dir_path)?;
	pgbar.set_draw_target(ProgressDrawTarget::stderr());

	for media_helper in /* utils::find_editable_files(download_path)? */ final_media.mediainfo_map.values() {
		pgbar.inc(1);
		let media = &media_helper.data;
		let (media_filename, final_filename) = match utils::convert_mediainfo_to_filename(media) {
			Some(v) => v,
			None => {
				warn!("Found MediaInfo which returned \"None\" from \"convert_mediainfo_to_filename\", skipping (id: \"{}\")", media.id);

				continue;
			},
		};
		// rename can be used, because it is a lower directory of the download_path, which should in 99.99% of cases be the same filesystem
		std::fs::rename(download_path.join(media_filename), final_dir_path.join(final_filename))?;
	}

	pgbar.finish_and_clear();

	debug!("Running Tagger");
	utils::run_editor(&sub_args.tagger_editor, &final_dir_path)?;

	return Ok(());
}

/// Try to find and read all recovery files in provided `path` and return the recovery files that were used
fn try_find_and_read_recovery_files(
	finished_media_vec: &mut MediaInfoArr,
	path: &Path,
) -> Result<Vec<PathBuf>, ioError> {
	if !path.is_dir() {
		return Err(ioError::new(
			std::io::ErrorKind::Other, // TODO: replace "Other" with "NotADirectory" when stable
			"Path to find recovery files is not existing or a directory!",
		));
	}

	let mut read_files: Vec<PathBuf> = Vec::new();
	// IMPORTANT: currently sysinfo creates threads, but never closes them (even when going out of scope)
	// see https://github.com/GuillaumeGomez/sysinfo/issues/927
	let mut s = sysinfo::System::new();
	s.refresh_processes();

	for file in path.read_dir()?.filter_map(|res| {
		let entry = res.ok()?;

		let path = entry.path();
		let file_name = path.file_name()?;
		if path.is_file() && file_name.to_string_lossy().starts_with(Recovery::RECOVERY_PREFIX) {
			return Some(path);
		}
		return None;
	}) {
		let file_name = file.file_name().unwrap().to_string_lossy(); // unwrap because non-file_name containing paths should be sorted out in the "filter_map"
		info!("Trying to read recovery file: \"{}\"", file_name);
		let pid_str = {
			let opt = file_name.split_once('_'); // `Recovery::RECOVERY_PREFIX` delimiter
			if opt.is_none() {
				continue;
			}
			opt.unwrap().1 // unwrap because "None" is checked above
		};
		let pid_of_file = {
			let res = pid_str.parse::<usize>();
			if res.is_err() {
				continue;
			}
			res.unwrap() // unwrap because "Err" is checked above
		};
		// check that the pid of the file is actually not running anymore
		// and just ignore them if the process exists
		if s.process(sysinfo::Pid::from(pid_of_file)).is_some() {
			info!("Found recovery file for pid {pid_of_file}, but the process still existed");
			continue;
		}
		// for now just add them regardless if they exist or not in the array
		for media in Recovery::read_recovery(&file)? {
			finished_media_vec.insert_with_comment(media, format!("From Recovery file of pid {pid_of_file}"));
		}
		read_files.push(file);
	}

	return Ok(read_files);
}

#[cfg(test)]
mod test {
	use super::*;

	mod recovery {
		use libytdlr::data::cache::media_provider::MediaProvider;

		use super::*;

		#[test]
		fn test_try_from_line() {
			// test a non-proper name
			let input = "impropername.something";
			assert_eq!(None, Recovery::try_from_line(input));

			// test a proper name
			let input = "'provider'-'id'-Some Title";
			assert_eq!(
				Some(
					MediaInfo::new("id")
						.with_provider(MediaProvider::Other("provider".to_owned()))
						.with_title("Some Title")
				),
				Recovery::try_from_line(input)
			);

			// test a proper name with dots
			let input = "'provider'-'id'-Some Title ver.2";
			assert_eq!(
				Some(
					MediaInfo::new("id")
						.with_provider(MediaProvider::Other("provider".to_owned()))
						.with_title("Some Title ver.2")
				),
				Recovery::try_from_line(input)
			);
		}
	}
}
