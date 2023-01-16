use crate::clap_conf::*;
use crate::state::DownloadState;
use crate::utils;
use diesel::SqliteConnection;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use libytdlr::{
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
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Static for easily referencing the 100% length for a progressbar
const PG_PERCENT_100: u64 = 100;
/// Static size the Download Progress Style will take (plus some spacers)
/// currently accounts for "[00/??] [00:00:00] ### "
const STYLE_STATIC_SIZE: usize = 23;

struct Recovery {
	// /// The path where the recovery file will be at
	// pub path:   PathBuf,
	/// The Writer to the file, open while this struct is not dropped
	pub writer: BufWriter<std::fs::File>,
}

impl Recovery {
	/// Open a new File at "path" and open a file handle
	pub fn create<P>(path: P, media_vec: &Vec<data::cache::media_info::MediaInfo>) -> std::io::Result<Self>
	where
		P: Into<PathBuf>,
	{
		let path = path.into();
		let writer = BufWriter::new(std::fs::File::create(path)?);
		let mut inst = Self { /* path, */ writer, };

		inst.write_recovery(media_vec)?;

		return Ok(inst);
	}

	/// Write the given MediaInfo-Vec to the file
	pub fn write_recovery(&mut self, media_vec: &Vec<data::cache::media_info::MediaInfo>) -> std::io::Result<()> {
		for media in media_vec.iter() {
			self.writer.write_all(Self::fmt_line(media).as_bytes())?;
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
	pub fn read_recovery(path: &Path) -> Result<Vec<data::cache::media_info::MediaInfo>, crate::Error> {
		if !path.exists() {
			return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "Recovery File Path does not exist").into());
		}
		// error in case of not being a file, maybe consider changeing this to a function and ignoring if not existing
		if !path.is_file() {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, "Recovery File Path is not a file").into());
		}
		let file_handle = BufReader::new(std::fs::File::open(path)?);

		let mut found_media_vec: Vec<data::cache::media_info::MediaInfo> = Vec::new();

		for media in file_handle
			.lines()
			.filter_map(|v| return v.ok())
			.filter_map(|v| return Self::try_from_line(&v))
		{
			found_media_vec.push(media);
		}

		return Ok(found_media_vec);
	}

	/// Tries to remove the given file, ignoring if the file does not exist and otherwise just logging the error
	pub fn remove_file(path: &Path) {
		std::fs::remove_file(path).unwrap_or_else(|err| match err.kind() {
			std::io::ErrorKind::NotFound => (),
			_ => info!("Error removing recovery file. Error: {}", err),
		});
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
pub fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), ioError> {
	utils::require_ytdl_installed()?;

	if sub_args.urls.is_empty() {
		return Err(ioError::new(std::io::ErrorKind::Other, "At least one URL is required"));
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

	let mut finished_media_vec = do_download(main_args, sub_args, &pgbar, &mut download_state)?;

	let download_path = download_state.get_download_path();
	let tmp_recovery_path = download_path.join("recovery");

	// TODO: rework how recovery works and only write recovery in case of error, also consider including state
	{
		if !finished_media_vec.is_empty() {
			info!("Saving downloaded media to temp storage for recovery");
			Recovery::create(&tmp_recovery_path, &finished_media_vec)?;
		} else {
			warn!("Trying to recover from tmp_recovery_path");

			if tmp_recovery_path.exists() {
				let mut media_vec = Recovery::read_recovery(&tmp_recovery_path)?;
				finished_media_vec.append(&mut media_vec);
			}
		}
	}

	let mut index = 0usize;
	// convert finished media elements to hashmap so it can be found without using a new iterator over and over
	let mut finished_media_map: HashMap<String, (usize, data::cache::media_info::MediaInfo)> = finished_media_vec
		.into_iter()
		.map(|v| {
			let res = (
				format!(
					"{}-{}",
					v.provider
						.as_ref()
						.map_or_else(|| return "unknown", |v| return v.to_str()),
					v.id
				),
				(index, v),
			);
			index += 1;
			return res;
		})
		.collect();

	// error-recovery, discover all files that can be edited, even if nothing has been downloaded
	// though for now it will not be in the download order
	if finished_media_map.is_empty() {
		debug!("Downloaded media was empty, trying to find editable files");
		// for safety reset the index variable
		let mut index = 0usize;
		finished_media_map = utils::find_editable_files(download_path)?
			.into_iter()
			.map(|v| {
				let res = (
					format!(
						"{}-{}",
						v.provider
							.as_ref()
							.map_or_else(|| return "unknown", |v| return v.to_str()),
						v.id
					),
					(index, v),
				);
				index += 1;
				return res;
			})
			.collect();
	} else {
		// merge found filenames into existing mediainfo
		for new_media in utils::find_editable_files(download_path)? {
			if let Some(media) = finished_media_map.get_mut(&format!(
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

				media.1.set_filename(new_media_filename);
			}
		}
	}

	// sort in index order
	let mut final_media_vec: Vec<(usize, data::cache::media_info::MediaInfo)> =
		finished_media_map.into_values().collect();
	final_media_vec.sort_by_key(|v| return v.0);

	edit_media(sub_args, download_path, final_media_vec)?;

	finish_media(sub_args, download_path)?;

	// do some cleanup
	// remove the recovery file, because of a successfull finish
	Recovery::remove_file(&tmp_recovery_path);

	return Ok(());
}

/// Do the download for all provided URL's
fn do_download(
	main_args: &CliDerive,
	sub_args: &CommandDownload,
	pgbar: &ProgressBar,
	download_state: &mut DownloadState,
) -> Result<Vec<data::cache::media_info::MediaInfo>, ioError> {
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

	let mut finished_media_vec: Vec<data::cache::media_info::MediaInfo> = Vec::new();

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

		finished_media_vec.extend(new_media);
	}

	// remove ytdl_archive_pid.txt file again, because otherwise over many usages it can become bloated
	std::fs::remove_file(libytdlr::main::download::get_archive_name(
		download_state.download_path(),
	))
	.unwrap_or_else(|err| {
		info!("Removing ytdl archive failed. Error: {}", err);
		return;
	});

	return Ok(finished_media_vec);
}

/// Start editing loop for all provided media
fn edit_media(
	sub_args: &CommandDownload,
	download_path: &std::path::Path,
	final_media_vec: Vec<(usize, data::cache::media_info::MediaInfo)>,
) -> Result<(), ioError> {
	// ask for editing
	// TODO: consider renaming before asking for edit
	'for_media_loop: for (_key, media) in /* utils::find_editable_files(download_path)? */ final_media_vec {
		let media_filename = match media.filename {
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
					"Edit Media \"{}\"?",
					media
						.title
						.as_ref()
						.expect("Expected MediaInfo to have a title from \"try_from_filename\"")
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
fn finish_media(sub_args: &CommandDownload, download_path: &std::path::Path) -> Result<(), ioError> {
	// the following is used to ask the user what to do with the media-files
	// current choices are:
	// move all media that is found to the final_directory (specified via options or defaulted), or
	// open picard and let picard handle the moving
	match utils::get_input("[m]ove Media to Output Directory or Open [p]icard?", &["m", "p"], "")?.as_str() {
		"m" => {
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

			for media in utils::find_editable_files(download_path)? {
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

			println!(
				"Moved {} media files to \"{}\"",
				moved_count,
				final_dir_path.to_string_lossy()
			);
		},
		"p" => {
			debug!("Renaming files for Picard");

			let final_dir_path = download_path.join("final");
			std::fs::create_dir_all(&final_dir_path)?;

			for media in utils::find_editable_files(download_path)? {
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

			debug!("Running Picard");
			utils::run_editor(&sub_args.picard_editor, &final_dir_path, false)?;
		},
		_ => unreachable!("get_input should only return a OK value from the possible array"),
	}

	return Ok(());
}
