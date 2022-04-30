#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate log;

use diesel::SqliteConnection;
use flexi_logger::LogSpecification;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use libytdlr::*;
use state::DownloadState;
use std::{
	cell::RefCell,
	collections::HashMap,
	fs::File,
	io::{
		BufReader,
		Error as ioError,
	},
	path::PathBuf,
};

mod clap_conf;
use clap_conf::*;

use crate::utils::{
	require_ffmpeg_installed,
	require_ytdl_installed,
};
mod logger;
mod state;
mod utils;

/// Main
fn main() -> Result<(), ioError> {
	let mut logger_handle = logger::setup_logger()?;

	let cli_matches = CliDerive::custom_parse();

	if cli_matches.debugger {
		warn!("Requesting Debugger");

		#[cfg(debug_assertions)]
		{
			invoke_vscode_debugger();
		}
		#[cfg(not(debug_assertions))]
		{
			println!("Debugger Invokation only available in Debug Target");
		}
	}

	log::info!("CLI Verbosity is {}", cli_matches.verbosity);

	// dont do anything if "-v" is not specified (use env / default instead)
	if cli_matches.verbosity > 0 {
		// apply cli "verbosity" argument to the log level
		logger_handle.set_new_spec(
			match cli_matches.verbosity {
				0 => unreachable!("Unreachable because it should be tested before that it is higher than 0"),
				1 => LogSpecification::parse("info"),
				2 => LogSpecification::parse("debug"),
				3 => LogSpecification::parse("trace"),
				_ => {
					return Err(ioError::new(
						std::io::ErrorKind::Other,
						"Expected verbosity integer range between 0 and 3 (inclusive)",
					))
				},
			}
			.expect("Expected LogSpecification to parse correctly"),
		);
	}

	match &cli_matches.subcommands {
		SubCommands::Download(v) => command_download(&cli_matches, v),
		SubCommands::Archive(v) => sub_archive(&cli_matches, v),
		SubCommands::ReThumbnail(v) => command_rethumbnail(&cli_matches, v),
	}?;

	return Ok(());
}

/// Handler function for the "archive" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn sub_archive(main_args: &CliDerive, sub_args: &ArchiveDerive) -> Result<(), ioError> {
	match &sub_args.subcommands {
		ArchiveSubCommands::Import(v) => command_import(main_args, v),
		// ArchiveSubCommands::Migrate(v) => command_migrate(main_args, v),
	}?;

	return Ok(());
}

/// Handler function for the "download" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_download(main_args: &CliDerive, sub_args: &CommandDownload) -> Result<(), ioError> {
	require_ytdl_installed()?;

	if sub_args.urls.is_empty() {
		return Err(ioError::new(std::io::ErrorKind::Other, "At least one URL is required"));
	}

	lazy_static::lazy_static! {
		// ProgressBar Style for download, will look like "[0/0] [00:00:00] [#>-] CustomMsg"
		static ref SINGLE_STYLE: ProgressStyle = ProgressStyle::default_bar()
		.template("{prefix:.dim} [{elapsed_precise}] {wide_bar:.cyan/blue} {msg}")
		.progress_chars("#>-");
	}

	// let mut errcode = false;
	let tmp_path = main_args
		.tmp_path
		.as_ref()
		.map_or_else(|| return std::env::temp_dir(), |v| return v.clone())
		.join("ytdl_rust_tmp");

	let pgbar: ProgressBar = ProgressBar::new(100).with_style(SINGLE_STYLE.clone());
	crate::utils::set_progressbar(&pgbar, main_args);
	let mut download_state = DownloadState::new(
		sub_args.audio_only_enable,
		sub_args.print_youtubedl_stdout,
		tmp_path,
		sub_args.force_genarchive_bydate,
		sub_args.force_genarchive_all,
	);
	let mut maybe_connection: Option<SqliteConnection> = {
		if let Some(ap) = main_args.archive_path.as_ref() {
			Some(crate::utils::handle_connect(ap, &pgbar, main_args)?.1)
		} else {
			None
		}
	};
	let download_info: RefCell<(usize, String, String)> = RefCell::new((0, String::default(), String::default()));
	pgbar.set_prefix(format!("[{}/{}]", "??", "??"));
	let download_pgcb = |dpg| match dpg {
		main::download::DownloadProgress::AllStarting => {
			pgbar.reset();
		},
		main::download::DownloadProgress::SingleStarting(id, title) => {
			let new_count = download_info.borrow().0 + 1;
			download_info.replace((new_count, id, title));

			pgbar.reset();
			let download_info_borrowed = download_info.borrow();
			pgbar.set_prefix(format!("[{}/{}]", download_info_borrowed.0, "??"));
			pgbar.set_message(format!("Downloading: {}", download_info_borrowed.2));
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
			pgbar.println(format!("Finished Downloading {} new Media", new_count));
		},
	};

	// TODO: do a "count" before running actual download

	let mut finished_vec_acc: Vec<data::cache::media_info::MediaInfo> = Vec::new();

	for url in &sub_args.urls {
		download_state.set_current_url(url);

		let new_media =
			libytdlr::main::download::download_single(maybe_connection.as_mut(), &download_state, download_pgcb)?;

		if let Some(ref mut connection) = maybe_connection {
			pgbar.reset();
			pgbar.set_length(new_media.len().try_into().expect("Failed to convert usize to u64"));
			for media in new_media.iter() {
				pgbar.inc(1);
				libytdlr::main::archive::import::insert_insmedia(&media.into(), connection)?;
			}
			pgbar.finish_and_clear();
		}

		finished_vec_acc.extend(new_media);
	}

	let download_path = download_state.get_download_path();

	// convert finished media elements to hashmap so it can be found without using a new iterator over and over
	let mut finished_vec_acc: HashMap<String, data::cache::media_info::MediaInfo> = finished_vec_acc
		.into_iter()
		.map(|v| {
			return (
				format!(
					"{}-{}",
					v.provider
						.as_ref()
						.map_or_else(|| return "unknown", |v| return v.to_str()),
					v.id
				),
				v,
			);
		})
		.collect();

	// merge found filenames into existing mediainfo
	for new_media in crate::utils::find_editable_files(download_path)? {
		if let Some(media) = finished_vec_acc.get_mut(&format!(
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

			media.set_filename(new_media_filename);
		}
	}

	// ask for editing
	// TODO: consider renaming before asking for edit
	'for_media_loop: for (_key, media) in /* crate::utils::find_editable_files(download_path)? */ finished_vec_acc {
		let media_filename = media
			.filename
			.expect("Expected MediaInfo to have a filename from \"try_from_filename\"");
		let media_path = download_path.join(&media_filename);
		// extra loop is required for printing the help and asking again
		'ask_do_loop: loop {
			let input = crate::utils::get_input(
				&format!(
					"Edit Media \"{}\"?",
					media
						.title
						.as_ref()
						.expect("Expected MediaInfo to have a title from \"try_from_filename\"")
				),
				&["h", "y", "N", "a", "v", "p"],
				"n",
			)?;

			match input.as_str() {
				"n" => continue 'for_media_loop,
				"y" => match crate::utils::get_filetype(&media_filename) {
					utils::FileType::Video => {
						println!("Found filetype to be of video");
						crate::utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?
					},
					utils::FileType::Audio => {
						println!("Found filetype to be of audio");
						crate::utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?
					},
					utils::FileType::Unknown => {
						// if not FileType could be found, ask user what to do
						match crate::utils::get_input(
							"Could not find suitable editor for extension, [a]udio editor, [v]ideo editor, a[b]ort, [n]ext.",
							&["a", "v", "b", "n"],
							"",
						)?
						.as_str()
						{
							"a" => crate::utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?,
							"v" => crate::utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?,
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
					[v] edit element with video editor\n\
					[p] play element with mpv\
					"
					);
					continue 'ask_do_loop;
				},
				"a" => {
					crate::utils::run_editor(&sub_args.audio_editor, &media_path, sub_args.print_editor_stdout)?;
				},
				"v" => {
					crate::utils::run_editor(&sub_args.video_editor, &media_path, sub_args.print_editor_stdout)?;
				},
				"p" => {
					// TODO: allow PLAYER to be something other than mpv
					crate::utils::run_editor(&Some(PathBuf::from("mpv")), &media_path, false)?;

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

	// the following is used to ask the user what to do with the media-files
	// current choices are:
	// move all media that is found to the final_directory (specified via options or defaulted), or
	// open picard and let picard handle the moving
	match crate::utils::get_input("[m]ove Media to Output Directory or Open [p]icard?", &["m", "p"], "")?.as_str() {
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

			for media in crate::utils::find_editable_files(download_path)? {
				let (media_filename, final_filename) = match crate::utils::convert_mediainfo_to_filename(&media) {
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
				std::fs::copy(&from_path, to_path)?;

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

			return Ok(());
		},
		"p" => {
			debug!("Renaming files for Picard");

			let final_dir_path = download_path.join("final");
			std::fs::create_dir_all(&final_dir_path)?;

			for media in crate::utils::find_editable_files(download_path)? {
				let (media_filename, final_filename) = match crate::utils::convert_mediainfo_to_filename(&media) {
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
			crate::utils::run_editor(&sub_args.picard_editor, &final_dir_path, false)?;

			return Ok(());
		},
		_ => unreachable!("get_input should only return a OK value from the possible array"),
	}
}

/// Handler function for the "archive import" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_import(main_args: &CliDerive, sub_args: &ArchiveImport) -> Result<(), ioError> {
	use libytdlr::main::archive::import::*;
	println!("Importing Archive from \"{}\"", sub_args.file_path.to_string_lossy());

	let input_path = &sub_args.file_path;

	if main_args.archive_path.is_none() {
		return Err(ioError::new(
			std::io::ErrorKind::Other,
			"Archive is required for Import!",
		));
	}

	let archive_path = main_args
		.archive_path
		.as_ref()
		.expect("Expected archive check to have already returned");

	lazy_static::lazy_static! {
		static ref IMPORT_STYLE: ProgressStyle = ProgressStyle::default_bar()
			.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
			.progress_chars("#>-");
	}

	let bar: ProgressBar = ProgressBar::hidden().with_style(IMPORT_STYLE.clone());
	crate::utils::set_progressbar(&bar, main_args);

	let (_new_archive, mut connection) = utils::handle_connect(archive_path, &bar, main_args)?;

	let mut reader = BufReader::new(File::open(input_path)?);

	let pgcb_import = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {} elements", v)),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Import"),
				ImportProgress::SizeHint(v) => println!("Import SizeHint: {}", v),
				ImportProgress::Increase(c, i) => println!("Import Increase: {}, Current Index: {}", c, i),
				ImportProgress::Finished(v) => println!("Import Finished, Successfull Imports: {}", v),
				_ => (),
			}
		}
	};

	import_any_archive(&mut reader, &mut connection, pgcb_import)?;

	return Ok(());
}

/// Handler function for the "archive migrate" subcommand
/// This function is mainly to keep the code structured and sorted
// #[inline]
// fn command_migrate(main_args: &CliDerive, sub_args: &ArchiveMigrate) -> Result<(), ioError> {}

/// Handler function for the "rethumbnail" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
fn command_rethumbnail(_main_args: &CliDerive, sub_args: &CommandReThumbnail) -> Result<(), ioError> {
	use libytdlr::main::rethumbnail::*;
	require_ffmpeg_installed()?;

	// helper aliases to make it easier to access
	let input_image_path: &PathBuf = &sub_args.input_image_path;
	let input_media_path: &PathBuf = &sub_args.input_media_path;
	let output_media_path: &PathBuf = sub_args
		.output_media_path
		.as_ref()
		.expect("Expected trait \"Check\" to be run on \"CommandReThumbnail\" before this point");

	println!(
		"Re-Applying Thumbnail image \"{}\" to media file \"{}\"",
		input_image_path.to_string_lossy(),
		input_media_path.to_string_lossy()
	);

	re_thumbnail_with_tmp(input_media_path, input_image_path, output_media_path)?;

	println!(
		"Re-Applied Thumbnail to media, as \"{}\"",
		output_media_path.to_string_lossy()
	);

	return Ok(());
}
