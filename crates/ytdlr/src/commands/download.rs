use crate::{
	clap_conf::{
		CliDerive,
		CommandDownload,
	},
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
	data,
	data::cache::media_info::MediaInfo,
	diesel,
	error::IOErrorToError,
	main,
	main::download::{
		SkippedType,
		YTDL_ARCHIVE_PREFIX,
	},
	traits::download_options::DownloadOptions,
};
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
	cell::{
		Cell,
		RefCell,
	},
	collections::HashMap,
	io::{
		BufRead,
		BufReader,
		BufWriter,
		Write,
	},
	path::{
		Path,
		PathBuf,
	},
	time::Duration,
};

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
		.termination_requested()
	{
		return Err(crate::Error::other("Termination Requested"));
	}

	return Ok(());
}

impl Recovery {
	/// Recovery file prefix
	const RECOVERY_PREFIX: &'static str = "recovery_";

	/// Create a new instance, without opening a file
	pub fn new<P>(path: P) -> Result<Self, crate::Error>
	where
		P: AsRef<Path>,
	{
		let path: PathBuf = libytdlr::utils::to_absolute(&path).attach_path_err(path)?; // absolutize the path so that "parent" does not return empty
		Self::check_path(&path)?; // check that the path is valid, and not only when trying to open it (when it would already be too late)
		return Ok(Self { path, writer: None });
	}

	/// Check a given path if it is valid to be wrote in
	fn check_path(path: &Path) -> Result<(), crate::Error> {
		// check that the given path does not already exist, as to not overwrite it
		if path.exists() {
			return Err(crate::Error::custom_ioerror_path(
				std::io::ErrorKind::AlreadyExists,
				"Recovery File Path already exists!",
				path,
			));
		}
		// check that the given path has a parent
		let parent = path.parent().ok_or_else(|| {
			return crate::Error::other("Failed to get the parent for the Recovery File!");
		})?;
		// check that the parent already exists
		if !parent.exists() {
			return Err(crate::Error::custom_ioerror_path(
				std::io::ErrorKind::NotFound,
				"Recovery File directory does not exist!",
				path,
			));
		}

		// check that the parent is writeable
		let meta = std::fs::metadata(parent).attach_path_err(parent)?;

		if meta.permissions().readonly() {
			return Err(crate::Error::custom_ioerror_path(
				std::io::ErrorKind::PermissionDenied,
				"Recovery File directory is not writeable!",
				parent,
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
			media.provider,
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

		return Some(data::cache::media_info::MediaInfo::new(&cap[2], &cap[1]).with_title(&cap[3]));
	}

	/// Try to read the recovery from the given path
	pub fn read_recovery(path: &Path) -> Result<impl Iterator<Item = MediaInfo>, crate::Error> {
		if !path.exists() {
			return Err(crate::Error::custom_ioerror_path(
				std::io::ErrorKind::NotFound,
				"Recovery File Path does not exist",
				path,
			));
		}
		// error in case of not being a file, maybe consider changeing this to a function and ignoring if not existing
		if !path.is_file() {
			return Err(crate::Error::custom_ioerror_path(
				std::io::ErrorKind::Other,
				"Recovery File Path is not a file",
				path,
			));
		}
		let file_handle = BufReader::new(std::fs::File::open(path).attach_path_err(path)?);

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
		std::fs::remove_file(path).unwrap_or_else(|err| {
			if err.kind() != std::io::ErrorKind::NotFound {
				info!("Error removing recovery file. Error: {}", err);
			}
		});
	}
}

/// Helper struct to preserve the order of download / addition and the data, with names
#[derive(Debug, PartialEq)]
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
#[derive(Debug, PartialEq)]
struct MediaInfoArr {
	/// Stores all [MediaHelper] and the keys are "provider-id"
	mediainfo_map:        HashMap<String, MediaHelper>,
	/// Stores the next "order" to be used for a new [MediaHelper]
	next_order:           usize,
	/// Store if the hashmap has maybe entries that are not in the archive
	has_maybe_uninserted: bool,
}

impl MediaInfoArr {
	/// Create a new empty instance
	pub fn new() -> Self {
		return Self {
			mediainfo_map:        HashMap::default(),
			next_order:           0,
			has_maybe_uninserted: false,
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
		self.has_maybe_uninserted = true;
		return self._insert(mediainfo, Some(comment.into()));
	}

	/// Helper for [`Self::insert`] and [`Self::insert_with_comment`] to only have one implementation
	fn _insert(&mut self, mediainfo: MediaInfo, comment: Option<String>) -> Option<MediaHelper> {
		let order = self.next_order;
		self.next_order += 1;

		let key = format!("{}-{}", mediainfo.provider.as_ref(), mediainfo.id);

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

	/// Get if the internal hashmap maybe has entries that are not inserted to the archive
	pub fn has_maybe_uninserted(&self) -> bool {
		return self.has_maybe_uninserted;
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
/// NOTE: this currently only gets run once for each "SingleStarting" instead of every tick, so truncation on resize will only happen at the next media
fn truncate_message_term_width<M>(msg: &M) -> String
where
	M: AsRef<str>,
{
	let display_width_available = terminal_size::terminal_size().map(|(w, _h)| {
		return (w.0 as usize).saturating_sub(STYLE_STATIC_SIZE);
	});

	let Some(display_width_available) = display_width_available else {
		return msg.as_ref().into();
	};

	return utils::truncate_message_display_pos(msg, display_width_available, true).to_string();
}

/// Find all files that match the temporary ytdl archive name, and remove all whose pid is not alive anymore
fn find_and_remove_tmp_archive_files(path: &Path) -> Result<(), crate::Error> {
	if !path.is_dir() {
		return Err(crate::Error::not_a_directory(
			"Path to find recovery files is not existing or a directory!",
			path,
		));
	}

	// IMPORTANT: currently sysinfo creates threads, but never closes them (even when going out of scope)
	// see https://github.com/GuillaumeGomez/sysinfo/issues/927
	let mut s = sysinfo::System::new();
	s.refresh_processes();

	for file in path.read_dir().attach_path_err(path)?.filter_map(|res| {
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

			let Some(cap) = cap else {
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
		std::fs::remove_file(file).unwrap_or_else(|err| {
			if err.kind() != std::io::ErrorKind::NotFound {
				info!("Error removing found tmp yt-dl archvie file. Error: {}", err);
			}
		});
	}

	return Ok(());
}

/// Handler function for the "download" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), crate::Error> {
	let ytdl_version = utils::require_ytdl_installed()?;

	let only_recovery = sub_args.urls.is_empty();

	if only_recovery {
		if sub_args.no_check_recovery {
			return Err(crate::Error::other("At least one URL is required"));
		}

		println!(
			"{} No URLs were provided, only checking recovery! To disable allowing 0 URLs, use \"--no-check-recovery\"",
			"WARN".color(Color::TrueColor { r: 255, g: 135, b: 0 })
		);
	}

	/// ProgressBar Style for download, will look like `[0/0] [00:00:00] [#>-] CustomMsg`
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

	std::fs::create_dir_all(&tmp_path).attach_path_err(&tmp_path)?;

	let pgbar: ProgressBar = ProgressBar::new(PG_PERCENT_100).with_style(DOWNLOAD_STYLE.clone());
	utils::set_progressbar(&pgbar, main_args);

	let mut download_state = DownloadState::new(sub_args, tmp_path, &ytdl_version);

	// already create the vec for finished media, so that the finished ones can be stored in case of error
	let mut finished_media = MediaInfoArr::new();
	let mut recovery = Recovery::new(download_state.download_path().join(format!(
		"{}{}",
		Recovery::RECOVERY_PREFIX,
		std::process::id()
	)))?;

	// recover files that are not in a recovery but are still considered editable
	// only do this in "only_recovery" mode (no urls) to not accidentally use from other processes
	if only_recovery {
		for media in utils::find_editable_files(download_state.download_path())? {
			finished_media.insert_with_comment(media, "Found Editable File");
		}
	}

	find_and_remove_tmp_archive_files(download_state.download_path())?;

	// run AFTER finding all files, so that the correct filename is already set for files, and only information gets updated
	let found_recovery_files = try_find_and_read_recovery_files(&mut finished_media, download_state.download_path())?;

	// TODO: consider cross-checking archive if the files from recovery are already in the archive and get a proper title

	match download_wrapper(
		main_args,
		sub_args,
		&pgbar,
		&mut download_state,
		&mut finished_media,
		only_recovery,
	) {
		Ok(()) => (),
		Err(err) => {
			let res = recovery.write_recovery(&finished_media);

			// log recovery write error, but do not modify original error
			if let Err(rerr) = res {
				warn!("Failed to write recovery: {}", rerr);
			}

			return Err(err);
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

/// Helper enum to decide what to do in the finish media loop (to not have to nest calls)
#[derive(Debug, PartialEq)]
enum EditCtrl {
	/// Indicate that the loop is finished
	Finished,
	/// Indicate that the loop should continue
	Goback,
}

/// Wrapper for [`command_download`] to house the part where in case of error a recovery needs to be written
fn download_wrapper(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	pgbar: &ProgressBar,
	download_state: &mut DownloadState,
	finished_media: &mut MediaInfoArr,
	only_recovery: bool,
) -> Result<(), crate::Error> {
	if only_recovery {
		info!("Skipping download because of \"only_recovery\"");
	} else {
		do_download(main_args, sub_args, pgbar, download_state, finished_media)?;
	}

	let download_path = download_state.download_path();
	// determines whether the "reverse" argument for "edit_media" is set
	let mut looped_once = false;

	// loop so that when selecting "b" in "finish_media" to be able to go back to editing
	loop {
		edit_media(main_args, sub_args, download_path, finished_media, looped_once)?;
		looped_once = true;

		match finish_media(main_args, sub_args, download_path, pgbar, finished_media)? {
			EditCtrl::Finished => break,
			EditCtrl::Goback => continue,
		}
	}

	return Ok(());
}

/// Characters to use if a state for the ProgressBar is unknown
const PREFIX_UNKNOWN: &str = "??";

/// Helper function to consistently set the progressbar prefix
fn set_progressbar_prefix(
	pgbar: &ProgressBar,
	download_info: &DownloadInfo,
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
		download_info.get_count_estimate().to_string()
	};
	pgbar.set_prefix(format!("[{}/{}]", current_count, playlist_count));
}

/// Set the default count estimate
const DEFAULT_COUNT_ESTIMATE: usize = 1;

/// NewType to store a count and a bool together
/// Where the count is the playlist size estimate and the bool for whether it has already been set to a non-default
/// values: (count_estimate, has_been_set, decrease_by)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountStore {
	count_estimate: usize,
	has_been_set:   bool,
	decrease_by:    usize,
}

impl CountStore {
	pub fn new(count_estimate: usize, has_been_set: bool, decrease_by: usize) -> Self {
		return Self {
			count_estimate,
			has_been_set,
			decrease_by,
		};
	}

	/// Get wheter a count set (non-default) has occured
	pub fn has_been_set(&self) -> bool {
		return self.has_been_set;
	}
}

/// Helper struct to keep track of some state, while having named fields instead of numbered tuple fields
///
/// This State contains state about the url position and playlist (inside one url) position
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
	/// Contains the value for the current playlist count estimate
	count_estimate:     Cell<CountStore>,
}

impl DownloadInfo {
	/// Create a new instance of [Self] with all the provided options
	pub fn new(playlist_count: usize, id: String, title: String, url_index: usize) -> Self {
		return Self {
			playlist_count,
			id,
			title,
			url_index,
			count_estimate: Cell::new(CountStore::new(DEFAULT_COUNT_ESTIMATE, false, 0)),
		};
	}

	/// Set "count_result" for generating the archive and for "get_count_estimate"
	/// this function will automatically decrease the count by "decrease_by" (`CountStore.2`)
	pub fn set_count_estimate(&self, count: usize) {
		let old_count = self.count_estimate.get();

		let new_count = count.saturating_sub(old_count.decrease_by);
		if new_count < DEFAULT_COUNT_ESTIMATE {
			self.count_estimate
				.replace(CountStore::new(DEFAULT_COUNT_ESTIMATE, true, 0));
		} else {
			self.count_estimate.replace(CountStore::new(new_count, true, 0));
		}
	}

	/// Reset the count estimate to default
	pub fn reset_count_estimate(&self) {
		self.count_estimate
			.replace(CountStore::new(DEFAULT_COUNT_ESTIMATE, false, 0));
	}

	/// Dedicated function to decrease the count estimate, even if no estimate has been given yet
	pub fn decrease_count_estimate(&self, decrease_by: usize) {
		let old_count = self.count_estimate.get();

		if old_count.has_been_set() {
			let mut new_count = old_count
				.count_estimate
				.saturating_sub(decrease_by)
				.saturating_sub(old_count.decrease_by);
			if new_count < DEFAULT_COUNT_ESTIMATE {
				new_count = DEFAULT_COUNT_ESTIMATE;
			}
			self.count_estimate
				.replace(CountStore::new(new_count, old_count.has_been_set, 0));
		} else {
			self.count_estimate.replace(CountStore::new(
				old_count.count_estimate,
				old_count.has_been_set,
				old_count.decrease_by + decrease_by,
			));
		}
	}

	/// Get the a copy of the current [CountStore]
	pub fn get_count_store(&self) -> CountStore {
		return self.count_estimate.get();
	}

	pub fn get_count_estimate(&self) -> usize {
		return self.count_estimate.get().count_estimate;
	}

	pub fn reset_new_starting(&mut self, playlist_count: usize, id: String, title: String, url_index: usize) {
		self.playlist_count = playlist_count;
		self.id = id;
		self.title = title;
		self.url_index = url_index;
	}

	pub fn reset_for_new_url(&mut self, url_index: usize) {
		self.playlist_count = 0;
		self.id = String::default();
		self.title = String::default();
		self.url_index = url_index
	}
}

impl Default for DownloadInfo {
	fn default() -> Self {
		return Self::new(0, String::default(), String::default(), 0);
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
	let mut maybe_connection: Option<SqliteConnection> = if let Some(ap) = main_args.archive_path.as_ref() {
		Some(utils::handle_connect(ap, pgbar, main_args)?.1)
	} else {
		None
	};

	// store "download_state" in a refcell, because rust complains that a borrow is made in "download_pgcb" and also later used while still in scope
	let download_state_cell: RefCell<&mut DownloadState> = RefCell::new(download_state);
	let download_info: RefCell<DownloadInfo> = RefCell::new(DownloadInfo::default());
	let url_len = sub_args.urls.len();
	set_progressbar_prefix(pgbar, &download_info.borrow(), true, true);
	// track total count finished (no error)
	let total_count = std::sync::atomic::AtomicUsize::new(0);
	let download_pgcb = |dpg| match dpg {
		main::download::DownloadProgress::AllStarting => {
			pgbar.reset();
			pgbar.set_message(""); // ensure it is not still present across finish and reset
			let url_index = download_info.borrow().url_index;
			download_info.borrow_mut().reset_for_new_url(url_index);
			download_info.borrow().reset_count_estimate(); // reset count estimate so that it does not carry over to different URLs
		},
		main::download::DownloadProgress::SingleStarting(id, title) => {
			let new_count = download_info.borrow().playlist_count + 1;
			let url_index = download_info.borrow().url_index;
			download_info
				.borrow_mut()
				.reset_new_starting(new_count, id, title, url_index);

			pgbar.reset();
			pgbar.set_length(PG_PERCENT_100); // reset length, because it may get changed because of connection insert
			let download_info_borrowed = download_info.borrow();
			set_progressbar_prefix(pgbar, &download_info.borrow(), false, false);
			// steady-ticks have to be re-done after every "pgbar.finish" because the ticker will exit once it notices the state is "finished"
			pgbar.enable_steady_tick(Duration::from_secs(1));
			pgbar.set_message(truncate_message_term_width(&download_info_borrowed.title));
			pgbar.println(format!("Downloading: {}", download_info_borrowed.title));
		},
		main::download::DownloadProgress::SingleProgress(_maybe_id, percent) => {
			pgbar.set_position(percent.into());
		},
		main::download::DownloadProgress::SingleFinished(_id) => {
			// dont hide the progressbar so that the cli does not appear to do nothing
			pgbar.reset();
			pgbar.println(format!("Finished Downloading: {}", download_info.borrow().title));
			set_progressbar_prefix(pgbar, &download_info.borrow(), false, false);
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
			let borrow = download_info.borrow();
			// only assign a playlist estimate count once for the current URL
			if !borrow.get_count_store().has_been_set() {
				borrow.set_count_estimate(new_count);
			}
		},
		// remove skipped medias from the count estimate (for the progress-bar)
		main::download::DownloadProgress::Skipped(skipped_count, skipped_type) => {
			download_info.borrow().decrease_count_estimate(skipped_count);

			// decrease playlist count too in case of error, because otherwise it could be playlist_count > count_estimate
			// like 20 > 10
			if skipped_type == SkippedType::Error {
				download_info.borrow_mut().playlist_count -= 1;
			}

			pgbar.reset(); // reset so that it can work both with "SingleStarting" happening or not
			   // set prefex so that the progressbar is shown while skipping elements, to not have the cli appear as "doing nothing"
			set_progressbar_prefix(
				pgbar,
				&download_info.borrow(),
				!download_info.borrow().get_count_store().has_been_set(),
				false,
			);
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

		// the array where finished "current_mediainfo" gets appended to
		// for performance / allocation efficiency, a count is requested from options
		let mut new_media: Vec<MediaInfo> = Vec::with_capacity(download_info.borrow().get_count_estimate());

		// dont error immediately on error
		let res = libytdlr::main::download::download_single(
			maybe_connection.as_mut(),
			*download_state_cell.borrow(),
			download_pgcb,
			&mut new_media,
		);

		// still add all finished media to the archive
		if let Some(ref mut connection) = maybe_connection {
			pgbar.reset();
			pgbar.set_length(new_media.len().try_into().expect("Failed to convert usize to u64"));
			for media in &new_media {
				pgbar.inc(1);
				if let Err(err) = libytdlr::main::archive::import::insert_insmedia(&media.into(), connection) {
					warn!("Inserting media errored: {}", err);
				}
			}
			pgbar.finish_and_clear();
		}

		// quick hint so that insertion is faster
		// because insertion is one element at a time
		finished_media.reserve(new_media.len());

		for media in new_media {
			finished_media.insert(media);
		}

		// now error if there was a error
		res?;
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
/// set "reverse" to start the editing on the last element
fn edit_media(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	final_media: &MediaInfoArr,
	reverse: bool,
) -> Result<(), crate::Error> {
	if !main_args.is_interactive() {
		info!("Skipping asking for media, because \"is_interactive\" is \"false\"");
		return Ok(());
	}

	if final_media.is_empty() {
		println!("Skipping asking for media, because there is no media to edit");
		return Ok(());
	}

	let media_sorted_vec = final_media.as_sorted_vec();
	let mut next_index = 0;

	if reverse {
		next_index = media_sorted_vec.len() - 1; // case of 0 - 1 should be solved by the "is_empty" above
	}

	// storage for when a element needs to be skipped (like missing filename) to know what should be done
	let mut go_back = false;

	// ask for editing
	// TODO: consider renaming before asking for edit
	'media_loop: loop {
		// handle terminate
		check_termination()?;

		// safety reset, because otherwise if element 0 is "skipped" (like no filename) then it would be a infinite loop
		if next_index == 0 {
			go_back = false;
		}

		let opt = media_sorted_vec.get(next_index);
		next_index += 1;

		let Some(media_helper) = opt else {
			break;
		};

		let media = &media_helper.data;
		let Some(media_filename) = &media.filename else {
			// skip asking edit for media's without a filename
			println!(
				"\"{}\" did not have a filename, which is required beyond this point, skipping",
				media.id
			);
			println!("debug: {media:#?}");

			// try to go back to the next element
			if go_back {
				next_index = next_index.saturating_sub(2);
			}

			continue 'media_loop;
		};

		let media_path = download_path.join(media_filename);

		// skip asking edit for media's that dont exist anymore
		if !media_path.exists() {
			println!(
				"\"{}\" did not exist anymore (moved via another invocation or editor rename?), skipping edit",
				media.id
			);

			// try to go back to the next element
			if go_back {
				next_index = next_index.saturating_sub(2);
			}

			continue 'media_loop;
		}

		go_back = false;
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
						.map_or(String::new(), |msg| format!(" ({msg})"))
				),
				&["h", "y", "N", "a", "v", "p", "b"],
				"n",
			)?;

			match input.as_str() {
				"n" => continue 'media_loop,
				"y" => match utils::get_filetype(media_filename) {
					utils::FileType::Video => {
						println!("Found filetype to be of video");
						run_editor_wrap(&sub_args.video_editor, &media_path)?;
					},
					utils::FileType::Audio => {
						println!("Found filetype to be of audio");
						run_editor_wrap(&sub_args.audio_editor, &media_path)?;
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
							"n" => continue 'media_loop,
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
					[v] edit element with video editor\n\
					[p] start the element with a media player\n\
					[b] go back a element\
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
				"b" => {
					// QOL message to notify that the earliest index is already in use
					if next_index == 1 {
						println!("Cannot go back further");
					}

					next_index = next_index.saturating_sub(2); // remove the "+1" for the next that was already added, and remove another 1 to get back to the last element

					go_back = true;

					continue 'media_loop;
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

			continue 'media_loop;
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
	use super::{
		utils,
		IOErrorToError,
		Lazy,
		Path,
		PathBuf,
	};
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

		let output = ffmpeg_cmd.output().attach_location_err("ffmpeg output")?;

		let exit_status = output.status;

		if !exit_status.success() {
			debug!("ffmpeg did not exist successfully, displaying log:");
			let output = String::from_utf8_lossy(&output.stderr);
			debug!("STDERR {}", output);

			let last_lines = output.lines().rev().take(5).collect::<String>();

			return Err(crate::Error::command_unsuccessful(format!(
				"FFMPEG metadata save command failed, code: {}, last lines:\n{}",
				exit_status.code().map_or("None".into(), |v| return v.to_string()),
				last_lines
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

		let output = ffmpeg_cmd.output().attach_location_err("ffmpeg output")?;

		let exit_status = output.status;

		if !exit_status.success() {
			debug!("ffmpeg did not exist successfully, displaying log:");
			let output = String::from_utf8_lossy(&output.stderr);
			debug!("STDERR {}", output);

			let last_lines = output.lines().rev().take(5).collect::<String>();

			return Err(crate::Error::command_unsuccessful(format!(
				"FFMPEG metadata apply command failed, code: {}, last lines:\n{}",
				exit_status.code().map_or("None".into(), |v| return v.to_string()),
				last_lines
			)));
		}

		// rename can be used here, because both files exist in the same directory
		std::fs::rename(&media_file_tmp, media_file).attach_path_err(media_file_tmp)?;

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
) -> Result<EditCtrl, crate::Error> {
	if final_media.mediainfo_map.is_empty() {
		println!("No files to move or tag");
		return Ok(EditCtrl::Finished);
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
		match utils::get_input(
			"[m]ove Media to Output Directory or Open [p]icard or go [b]ack to editing?",
			&["m", "p", "b"],
			"",
		)?
		.as_str()
		{
			"m" => finish_with_move(sub_args, download_path, pgbar, final_media)?,
			"p" => finish_with_tagger(sub_args, download_path, pgbar, final_media)?,
			"b" => return Ok(EditCtrl::Goback),
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

	// try to insert media into the archive, if media has maybe not been inserted yet
	if final_media.has_maybe_uninserted() {
		let mut maybe_connection: Option<SqliteConnection> = if let Some(ap) = main_args.archive_path.as_ref() {
			Some(utils::handle_connect(ap, pgbar, main_args)?.1)
		} else {
			None
		};

		if let Some(ref mut connection) = maybe_connection {
			pgbar.reset();
			pgbar.set_length(
				final_media
					.mediainfo_map
					.len()
					.try_into()
					.expect("Failed to convert usize to u64"),
			);
			pgbar.set_message("Inserting missing Entries to Archive");
			for media in final_media.mediainfo_map.values() {
				let media = &media.data;
				pgbar.inc(1);
				libytdlr::main::archive::import::insert_insmedia_noupdate(&media.into(), connection)?;
			}
			pgbar.finish_and_clear();
		}
	}

	// notify the user if there are still files that have not been moved
	if !utils::find_editable_files(download_path)?.is_empty() {
		println!("{} Found Editable file that have not been moved.\nConsider running recovery mode if no other ytdlr is running (with 0 URLs)", "WARN".color(Color::TrueColor { r: 255, g: 135, b: 0 }));
	}

	return Ok(EditCtrl::Finished);
}

/// Options to easily change the max amount of numbered files before giving up
const MAX_NUMBERED_FILES: usize = 30;

/// Check output path of the combined "dir_path" and "filename"
/// if it exists, append up to "30" to it
/// if the output path still exists after "30", returns [None]
fn try_gen_final_path(dir_path: &Path, filename: &Path) -> Option<PathBuf> {
	let mut to_path = dir_path.join(filename);

	if to_path.exists() {
		warn!(
			"Initial \"to\" path already exists, trying to find a solution, file: \"{}\"",
			filename.display()
		);
		// ensure it does not run infinitely
		let mut i = 0;

		let Some(file_base) = filename.file_stem() else {
			error!("File did not have a file_stem!");
			return None;
		};
		let ext = filename.extension();

		while to_path.exists() && i < MAX_NUMBERED_FILES {
			i += 1;

			let name = {
				let mut name = file_base.to_owned();

				name.push(format!(" {}", i));

				if let Some(ext) = ext {
					// having to manually push "." because not "set_extension" exists for "OsString"
					name.push(".");
					name.push(ext);
				}

				name
			};

			to_path = dir_path.join(name);
		}

		if !to_path.exists() && i >= MAX_NUMBERED_FILES {
			error!(
				"Not moving file, because it already exists, and also {} more combinations! File: \"{}\"",
				MAX_NUMBERED_FILES,
				filename.display()
			);

			return None;
		}
	}

	return Some(to_path);
}

/// Move all media in `final_media` to it final resting place in `download_path`
/// Helper to separate out the possible paths
fn finish_with_move(
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), crate::Error> {
	debug!("Moving all files to the final destination");

	let final_dir_path = sub_args.output_path.as_ref().map_or_else(
		|| {
			return dirs::download_dir()
				.unwrap_or_else(|| return PathBuf::from("."))
				.join("ytdlr-out");
		},
		|v| return v.clone(),
	);
	std::fs::create_dir_all(&final_dir_path).attach_path_err(&final_dir_path)?;

	let mut moved_count = 0usize;
	pgbar.set_draw_target(ProgressDrawTarget::stderr());

	for media_helper in final_media.mediainfo_map.values() {
		pgbar.inc(1);
		let media = &media_helper.data;
		let Some((media_filename, final_filename)) = utils::convert_mediainfo_to_filename(media) else {
			warn!(
				"Found MediaInfo which returned \"None\" from \"convert_mediainfo_to_filename\", skipping (id: \"{}\")",
				media.id
			);

			continue;
		};
		let from_path = download_path.join(media_filename);
		let Some(to_path) = try_gen_final_path(&final_dir_path, &final_filename) else {
			continue; // file will be found again in the next run via recovery
		};
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
		std::fs::remove_file(&from_path).attach_path_err(from_path)?;

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
fn finish_with_tagger(
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), crate::Error> {
	debug!("Renaming files for Tagger");

	let final_dir_path = download_path.join("final");
	std::fs::create_dir_all(&final_dir_path).attach_path_err(&final_dir_path)?;
	pgbar.set_draw_target(ProgressDrawTarget::stderr());

	for media_helper in final_media.mediainfo_map.values() {
		pgbar.inc(1);
		let media = &media_helper.data;
		let Some((media_filename, final_filename)) = utils::convert_mediainfo_to_filename(media) else {
			warn!(
				"Found MediaInfo which returned \"None\" from \"convert_mediainfo_to_filename\", skipping (id: \"{}\")",
				media.id
			);

			continue;
		};
		// rename can be used, because it is a lower directory of the download_path, which should in 99.99% of cases be the same filesystem
		let from_path = download_path.join(media_filename);
		let Some(to_path) = try_gen_final_path(&final_dir_path, &final_filename) else {
			continue; // file will be found again in the next run via recovery
		};
		std::fs::rename(&from_path, to_path).attach_path_err(from_path)?;
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
) -> Result<Vec<PathBuf>, crate::Error> {
	if !path.is_dir() {
		return Err(crate::Error::not_a_directory(
			"Path for recovery files was not a directory!",
			path,
		));
	}

	let mut read_files: Vec<PathBuf> = Vec::new();
	// IMPORTANT: currently sysinfo creates threads, but never closes them (even when going out of scope)
	// see https://github.com/GuillaumeGomez/sysinfo/issues/927
	let mut s = sysinfo::System::new();
	s.refresh_processes();

	for file in path.read_dir().attach_path_err(path)?.filter_map(|res| {
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
		let pid_of_file = match pid_str.parse::<usize>() {
			Err(_) => continue,
			Ok(v) => v,
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

	// recovery files dont contain the file path, so find editable file and merge them
	for new_media in utils::find_editable_files(path)? {
		if let Some(media) = finished_media_vec.get_mut(&format!("{}-{}", new_media.provider.as_ref(), new_media.id)) {
			let new_media_filename = new_media
				.filename
				.expect("Expected MediaInfo to have a filename from \"find_editable_files\"");

			media.data.set_filename(new_media_filename);
		}
	}

	return Ok(read_files);
}

#[cfg(test)]
mod test {
	use super::*;

	mod recovery {
		use super::*;

		#[test]
		fn test_try_from_line() {
			// test a non-proper name
			let input = "impropername.something";
			assert_eq!(None, Recovery::try_from_line(input));

			// test a proper name
			let input = "'provider'-'id'-Some Title";
			assert_eq!(
				Some(MediaInfo::new("id", "provider").with_title("Some Title")),
				Recovery::try_from_line(input)
			);

			// test a proper name with dots
			let input = "'provider'-'id'-Some Title ver.2";
			assert_eq!(
				Some(MediaInfo::new("id", "provider").with_title("Some Title ver.2")),
				Recovery::try_from_line(input)
			);
		}
	}

	mod try_gen_final_path {
		use super::*;
		use std::fs::{
			rename,
			File,
		};
		use tempfile::{
			Builder as TempBuilder,
			TempDir,
		};

		fn create_tmp_dir() -> (PathBuf, TempDir) {
			let testdir = TempBuilder::new()
				.prefix("ytdl-test-try_gen_final_path-")
				.tempdir()
				.expect("Expected a temp dir to be created");

			return (testdir.as_ref().to_owned(), testdir);
		}

		#[test]
		fn test_no_rename() {
			let (dir, _tempdir) = create_tmp_dir();

			let input_dir = dir.join("input");
			std::fs::create_dir_all(&input_dir).unwrap();

			let testfile1 = input_dir.join("hello.mkv");
			let testfile2 = input_dir.join("another.mkv");

			File::create(&testfile1).unwrap();
			File::create(&testfile2).unwrap();

			let output_dir = dir.join("output");
			std::fs::create_dir_all(&output_dir).unwrap();

			{
				let gen = try_gen_final_path(&output_dir, Path::new(testfile1.file_name().unwrap())).unwrap();
				assert_eq!(output_dir.join(testfile1.file_name().unwrap()), gen);
				rename(testfile1, gen).unwrap();
			}
			{
				let gen = try_gen_final_path(&output_dir, Path::new(testfile2.file_name().unwrap())).unwrap();
				assert_eq!(output_dir.join(testfile2.file_name().unwrap()), gen);
				rename(testfile2, gen).unwrap();
			}
		}

		#[test]
		fn test_rename_simple() {
			let (dir, _tempdir) = create_tmp_dir();

			let input_dir = dir.join("input");
			std::fs::create_dir_all(&input_dir).unwrap();

			let testfile1 = input_dir.join("1-hello.mkv");

			File::create(&testfile1).unwrap();

			let output_dir = dir.join("output");
			std::fs::create_dir_all(&output_dir).unwrap();

			{
				let gen = try_gen_final_path(&output_dir, Path::new("hello.mkv")).unwrap();
				assert_eq!(output_dir.join("hello.mkv"), gen);
				rename(&testfile1, gen).unwrap();
			}

			{
				let gen = try_gen_final_path(&output_dir, Path::new("hello.mkv")).unwrap();
				assert_eq!(output_dir.join("hello 1.mkv"), gen);
			}
		}

		#[test]
		fn test_30_times() {
			let (dir, _tempdir) = create_tmp_dir();

			let input_dir = dir.join("input");
			std::fs::create_dir_all(&input_dir).unwrap();
			let output_dir = dir.join("output");
			std::fs::create_dir_all(&output_dir).unwrap();

			let mut vals = Vec::new();

			for i in 0..31 {
				let testfile = input_dir.join(format!("{}-hello.mkv", i));
				File::create(&testfile).unwrap();

				let res = try_gen_final_path(&output_dir, Path::new("hello.mkv"));

				vals.push(res.is_some());

				// rename so that the files actually exist
				if let Some(v) = &res {
					rename(testfile, v).unwrap();
				}

				// the 0th time it will not have numbers appended
				if i == 0 {
					assert_eq!(Some(output_dir.join("hello.mkv")), res);
				}

				// should loop 30 times to find a suitable name
				if (1..30).contains(&i) {
					assert_eq!(Some(output_dir.join(format!("hello {i}.mkv"))), res);
				}

				// the 31th time it should return "None" because it only checks 30 times
				if i >= 31 {
					assert!(res.is_none());
				}
			}

			assert_eq!(vals.len(), 31);
			assert_eq!(
				vals.iter().fold(0, |acc, v| {
					if *v {
						return acc + 1;
					}

					return acc;
				}),
				30
			);
			assert_eq!(
				vals.iter().fold(0, |acc, v| {
					if !*v {
						return acc + 1;
					}

					return acc;
				}),
				1
			);
		}
	}
}
