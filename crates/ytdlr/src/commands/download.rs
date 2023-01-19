use crate::clap_conf::*;
use crate::state::DownloadState;
use crate::utils;
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
	data::cache::media_info::MediaInfo,
	traits::context::DownloadOptions,
	*,
};
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
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Static for easily referencing the 100% length for a progressbar
const PG_PERCENT_100: u64 = 100;
/// Static size the Download Progress Style will take (plus some spacers)
/// currently accounts for "[00/??] [00:00:00] ### "
const STYLE_STATIC_SIZE: usize = 23;

struct Recovery {
	/// The path where the recovery file will be at
	pub path: PathBuf,
	/// The Writer to the file, open while this struct is not dropped
	writer:   Option<BufWriter<std::fs::File>>,
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
	pub fn write_recovery(&mut self, media_arr: &MediaInfoArr) -> std::io::Result<()> {
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
		lazy_static::lazy_static! {
			// Regex for getting the provider,id,title from a line in a recovery format
			// cap1: provider, cap2: id, cap3: title
			static ref FROM_LINE_REGEX: Regex = Regex::new(r"(?mi)^'([^']+)'-'([^']+)'-(.+)$").unwrap();
		}

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

		vec.sort_by(|a, b| a.order.cmp(&b.order));

		return vec;
	}
}

/// Truncate the given message to a lower size so that the progressbar does not do new-lines
/// truncation is required because indicatif would do new-lines, and adding truncation would only work with a (static) maximum size
/// NOTE: this currently only gets run once for each "SingleStartin" instead of every tick, so resizing the truncate will not be done (until next media)
fn truncate_message<'a, M>(msg: &'a M) -> String
where
	M: AsRef<str>,
{
	let msg = msg.as_ref();

	let characters_end_idx: usize;

	// get all characters and their boundaries
	let (characters, characters_highest_display) = {
		let mut display_position = 0; // keep track of the actual displayed position
		(
			msg.grapheme_indices(true)
				.map(|(i, s)| {
					display_position += s.width();
					return (i, s.len(), display_position);
				})
				.collect::<Vec<(usize, usize, usize)>>(),
			display_position,
		)
	};

	// cache ".len" because it does not need to be executed often
	let characters_len = characters.len();

	if let Some((w, _h)) = term_size::dimensions() {
		let width_available = w.saturating_sub(STYLE_STATIC_SIZE);
		// if the width_available is more than the message, use the full message
		// otherwise use "width_available"
		if characters_highest_display <= width_available {
			characters_end_idx = characters_len; // use full length of msg
		} else {
			// find the closest "display_position" length from the back
			characters_end_idx = characters
				.iter()
				.rev()
				.position(|(_pos, _len, dis)| return *dis <= width_available)
				.map(|v| return characters.len() - v) // substract "v" because ".rev().position()" counts *encountered elements* instead of actual index
				.unwrap_or(characters_len);
		}
	} else {
		// if no terminal dimesions are available, use the full message
		characters_end_idx = characters_len;
	}

	// get the char boundary for the last character's end
	let msg_end_idx = {
		let char = characters[characters_end_idx - 1];
		char.0 + char.1
	};

	let mut ret = String::from(&msg[0..msg_end_idx]);

	// replace the last 3 characters with "..." to indicate a truncation
	if ret.len() < msg.len() {
		ret.replace_range(ret.len() - 3..ret.len(), "...");
	}

	return ret;
}

/// Handler function for the "download" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), crate::Error> {
	utils::require_ytdl_installed()?;

	let only_recovery = sub_args.urls.is_empty();

	if only_recovery {
		if sub_args.no_check_recovery {
			return Err(ioError::new(std::io::ErrorKind::Other, "At least one URL is required").into());
		}

		println!(
			"{} No URLs were provided, only checking recovery! To disable allowing 0 URLs, use \"--no-check-recovery\"",
			"WARN".color(Color::TrueColor { r: 255, g: 135, b: 0 })
		)
	}

	lazy_static::lazy_static! {
		// ProgressBar Style for download, will look like "[0/0] [00:00:00] [#>-] CustomMsg"
		static ref DOWNLOAD_STYLE: ProgressStyle = ProgressStyle::default_bar()
		.template("{prefix:.dim} [{elapsed_precise}] {wide_bar:.cyan/blue} {msg}")
		.expect("Expected ProgressStyle template to be valid")
		.progress_chars("#>-");
	}

	let tmp_path = main_args
		.tmp_path
		.as_ref()
		.map_or_else(|| return std::env::temp_dir(), |v| return v.clone())
		.join("ytdl_rust_tmp");

	let pgbar: ProgressBar = ProgressBar::new(PG_PERCENT_100).with_style(DOWNLOAD_STYLE.clone());
	utils::set_progressbar(&pgbar, main_args);

	let mut download_state = DownloadState::new(
		sub_args.audio_only_enable,
		sub_args.print_youtubedl_stdout,
		tmp_path,
		sub_args.force_genarchive_bydate,
		sub_args.force_genarchive_all,
		sub_args.force_no_archive,
	);

	// already create the vec for finished media, so that the finished ones can be stored in case of error
	let mut finished_media = MediaInfoArr::new();
	let mut recovery = Recovery::new(download_state.get_download_path().join(format!(
		"{}{}",
		Recovery::RECOVERY_PREFIX,
		std::process::id()
	)))?;

	let found_recovery_files =
		try_find_and_read_recovery_files(&mut finished_media, download_state.get_download_path())?;

	// TODO: consider cross-checking archive if the files from recovery are already in the archive and get a proper title
	// TODO: consider finding files with proper extension and add them ("utils::find_editable_files(download_path)")

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
		do_download(main_args, sub_args, &pgbar, download_state, finished_media)?;
	} else {
		info!("Skipping download because of \"only_recovery\"");
	}

	let download_path = download_state.get_download_path();

	// // error-recovery, discover all files that can be edited, even if nothing has been downloaded
	// // though for now it will not be in the download order
	// if finished_media_map.is_empty() {
	// 	debug!("Downloaded media was empty, trying to find editable files");
	// 	// for safety reset the index variable
	// 	let mut index = 0usize;
	// 	finished_media_map = utils::find_editable_files(download_path)?
	// 		.into_iter()
	// 		.map(|v| {
	// 			let res = (
	// 				format!(
	// 					"{}-{}",
	// 					v.provider
	// 						.as_ref()
	// 						.map_or_else(|| return "unknown", |v| return v.to_str()),
	// 					v.id
	// 				),
	// 				(index, v),
	// 			);
	// 			index += 1;
	// 			return res;
	// 		})
	// 		.collect();
	// } else {
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
	// }

	edit_media(main_args, sub_args, download_path, finished_media)?;

	finish_media(main_args, sub_args, download_path, pgbar, finished_media)?;

	return Ok(());
}

/// Do the download for all provided URL's
fn do_download(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	pgbar: &ProgressBar,
	download_state: &mut DownloadState,
	finished_media: &mut MediaInfoArr,
) -> Result<(), ioError> {
	let mut maybe_connection: Option<SqliteConnection> = {
		if let Some(ap) = main_args.archive_path.as_ref() {
			Some(utils::handle_connect(ap, pgbar, main_args)?.1)
		} else {
			None
		}
	};

	// track (currentCountTried, currentId, currentTitle)
	// *currentCountTried does not include media already in archive
	let download_info: RefCell<(usize, String, String)> = RefCell::new((0, String::default(), String::default()));
	pgbar.set_prefix(format!("[{}/{}]", "??", "??"));
	// track total count finished (no error)
	let total_count = std::sync::atomic::AtomicUsize::new(0);
	let download_pgcb = |dpg| match dpg {
		main::download::DownloadProgress::AllStarting => {
			pgbar.reset();
			pgbar.set_message(""); // ensure it is not still present across finish and reset
		},
		main::download::DownloadProgress::SingleStarting(id, title) => {
			let new_count = download_info.borrow().0 + 1;
			download_info.replace((new_count, id, title));

			pgbar.reset();
			pgbar.set_length(PG_PERCENT_100); // reset length, because it may get changed because of connection insert
			let download_info_borrowed = download_info.borrow();
			pgbar.set_prefix(format!("[{}/{}]", download_info_borrowed.0, "??"));
			pgbar.set_message(truncate_message(&download_info_borrowed.2));
			pgbar.println(format!("Downloading: {}", download_info_borrowed.2));
		},
		main::download::DownloadProgress::SingleProgress(_maybe_id, percent) => {
			pgbar.set_position(percent.into());
		},
		main::download::DownloadProgress::SingleFinished(_id) => {
			pgbar.finish_and_clear();
			pgbar.println(format!("Finished Downloading: {}", download_info.borrow().2));
			// pgbar.finish_with_message();
		},
		main::download::DownloadProgress::AllFinished(new_count) => {
			pgbar.finish_and_clear();
			let total = total_count.fetch_add(new_count, std::sync::atomic::Ordering::AcqRel) + new_count;
			// print how many media has been downloaded since last "AllStarting" and how many in total in this run
			pgbar.println(format!(
				"Finished Downloading {new_count} new Media (For a total of {total} Media)"
			));
		},
	};

	// TODO: do a "count" before running actual download

	for url in &sub_args.urls {
		download_state.set_current_url(url);

		let new_media =
			libytdlr::main::download::download_single(maybe_connection.as_mut(), download_state, download_pgcb)?;

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
		download_state.download_path(),
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
) -> Result<(), ioError> {
	if !main_args.is_interactive() {
		info!("Skipping asking for media, because \"is_interactive\" is \"false\"");
		return Ok(());
	}

	let media_sorted_vec = final_media.as_sorted_vec();
	// ask for editing
	// TODO: consider renaming before asking for edit
	'for_media_loop: for media_helper in media_sorted_vec.iter() {
		let media = &media_helper.data;
		let media_filename = match &media.filename {
			Some(v) => v,
			None => {
				println!("\"{}\" did not have a filename!", media.id);
				println!("debug: {media:#?}");
				continue 'for_media_loop;
			},
		};
		let media_path = download_path.join(&media_filename);
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
				&["h", "y", "N", "a", "v" /* , "p" */],
				"n",
			)?;

			match input.as_str() {
				"n" => continue 'for_media_loop,
				"y" => match utils::get_filetype(&media_filename) {
					utils::FileType::Video => {
						println!("Found filetype to be of video");
						utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?
					},
					utils::FileType::Audio => {
						println!("Found filetype to be of audio");
						utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?
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
							"a" => utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?,
							"v" => utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?,
							"b" => return Err(crate::Error::Other("Abort Selected".to_owned()).into()),
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
					utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?;
				},
				"v" => {
					utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?;
				},
				// "p" => {
				// 	// TODO: allow PLAYER to be something other than mpv
				// 	utils::run_editor(&Some(PathBuf::from("mpv")), &media_path, false)?;

				// 	// re-do the loop, because it was only played
				// 	continue 'ask_do_loop;
				// },
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

/// Finish the given media by either opening up the tagger or moving to final destination
fn finish_media(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	pgbar: &ProgressBar,
	final_media: &MediaInfoArr,
) -> Result<(), ioError> {
	// first set the draw-target so that any subsequent setting change does not cause a draw
	pgbar.set_draw_target(ProgressDrawTarget::hidden()); // so that it stays hidden until actually doing stuff
	pgbar.reset();
	pgbar.set_length(final_media.mediainfo_map.len().try_into().unwrap_or(u64::MAX));
	pgbar.set_message("Moving files");

	if main_args.is_interactive() && !sub_args.open_tagger {
		// the following is used to ask the user what to do with the media-files
		// current choices are:
		// move all media that is found to the final_directory (specified via options or defaulted), or
		// open picard and let picard handle the moving
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
		let (media_filename, final_filename) = match utils::convert_mediainfo_to_filename(&media) {
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
	debug!("Renaming files for Picard");

	let final_dir_path = download_path.join("final");
	std::fs::create_dir_all(&final_dir_path)?;
	pgbar.set_draw_target(ProgressDrawTarget::stderr());

	for media_helper in /* utils::find_editable_files(download_path)? */ final_media.mediainfo_map.values() {
		pgbar.inc(1);
		let media = &media_helper.data;
		let (media_filename, final_filename) = match utils::convert_mediainfo_to_filename(&media) {
			Some(v) => v,
			None => {
				warn!("Found MediaInfo which returned \"None\" from \"convert_mediainfo_to_filename\", skipping (id: \"{}\")", media.id);

				continue;
			},
		};
		// rename can be used, because it is a lower directory of the download_path, which should in 99.99% of cases be the same directory
		std::fs::rename(download_path.join(media_filename), final_dir_path.join(final_filename))?;
	}

	pgbar.finish_and_clear();

	debug!("Running Picard");
	utils::run_editor(&sub_args.picard_editor, &final_dir_path, false)?;

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
	let mut s = sysinfo::System::new();
	s.refresh_processes();

	for file in path.read_dir()?.filter_map(|res| {
		let entry = res.ok()?;

		let path = entry.path();
		let file_name = path.file_name()?;
		if path.is_file() && Path::new(file_name).starts_with(Recovery::RECOVERY_PREFIX) {
			return Some(path);
		}
		return None;
	}) {
		let file_name = file.file_name().unwrap().to_string_lossy(); // unwrap because non-file_name containing paths should be sorted out in the "filter_map"
		info!("Trying to read recovery file: \"{}\"", file_name);
		let pid_str = {
			let opt = file_name.split_once("_"); // `Recovery::RECOVERY_PREFIX` delimiter
			if opt.is_none() {
				continue;
			}
			opt.unwrap().1 // unwrap because "None" is checked above
		};
		let pid_of_file = {
			let res = usize::from_str_radix(pid_str, 10);
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
