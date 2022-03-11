use super::utils::{
	Arguments,
	LineTypes,
};
use crate::data::{
	provider::Provider,
	video::Video,
};

use colored::Colorize;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use regex::Regex;
use std::fs::File;
use std::io::{
	BufRead,
	BufReader,
	Error as ioError,
	ErrorKind,
};
use std::path::{
	Path,
	PathBuf,
};
use std::process::Stdio;

lazy_static! {
	static ref YTDL_ERROR: Regex = Regex::new(r"(?mi)^ERROR").unwrap();
	static ref SINGLE_STYLE: ProgressStyle = ProgressStyle::default_bar()
		.template("{prefix:.dim} [{elapsed_precise}] {wide_bar:.cyan/blue} {msg}")
		.progress_chars("#>-");
}

/// shorthand for unwrapping or early-returning
#[macro_export]
macro_rules! unwrap_or_return {
	($e:expr) => {
		match $e {
			Some(v) => v,
			None => return,
		}
	};
}

/// format the prefix
#[inline]
fn prefix_format<T: AsRef<str>>(current: &usize, count: &usize, id: T) -> String {
	if id.as_ref().is_empty() {
		return format!("[{}/{}]", &current, &count);
	}

	return format!("[{}/{}] ({})", &current, &count, id.as_ref());
}

/// Spawn the main Youtube-dl task
pub fn spawn_ytdl(args: &mut Arguments) -> Result<(), ioError> {
	use crate::main::count::*;
	let count_video = count(&args.url)
		.map_err(|err| return std::io::Error::new(ErrorKind::Other, format!("{}", err)))?
		.len();
	let mut current_video: usize = 0;

	let mut ytdl = crate::spawn::ytdl::base_ytdl();
	// it needs to be a string, otherwise the returns would complain about not living long enough
	let tmpdir = Path::new(&args.tmp).join("%(title)s.%(ext)s");

	if args.audio_only {
		ytdl.arg("-f");
		ytdl.arg("bestaudio/best");
		ytdl.arg("-x");
		ytdl.arg("--audio-format");
		ytdl.arg("mp3");
		ytdl.arg("--embed-thumbnail");
		ytdl.arg("--add-metadata");
	}

	if let Some(archive) = &args.archive {
		let archive_tmp = PathBuf::from(&tmpdir)
			.parent()
			.expect("Couldnt get Parent from tmpdir!")
			.join("ytdl_archive.txt");

		{
			let mut archive_handle = File::create(&archive_tmp).expect("Couldnt open archive_tmp path!");

			archive
				.to_ytdl_archive(&mut archive_handle)
				.expect("Couldnt Write to archive_tmp file!");
		}

		ytdl.arg("--download-archive").arg(&archive_tmp);
	}

	ytdl.arg("--newline"); // to make parsing easier

	// yt-dlp argument to always print in a specific format when available
	ytdl.arg("--print");
	ytdl.arg("PARSE '%(extractor)s' '%(id)s' %(title)s");

	// always enable printing progress
	ytdl.arg("--progress");
	// ensure progress gets printed and recieved properly
	ytdl.arg("--newline");

	// always disable "simulate" from enabling
	ytdl.arg("--no-simulate");

	ytdl.arg("-o").arg(tmpdir);
	for arg in args.extra_args.iter() {
		ytdl.arg(arg);
	}

	ytdl.arg(&args.url);

	let bar: ProgressBar = ProgressBar::new(100).with_style(SINGLE_STYLE.clone());

	if args.debug {
		bar.println("Printing YTDL raw-Output");
		bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
		bar.tick();
	}

	let mut spawned_ytdl = ytdl
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()?;

	let reader_stdout = BufReader::new(spawned_ytdl.stdout.take().expect("couldnt get stdout of Youtube-DL"));
	let reader_stderr = BufReader::new(spawned_ytdl.stderr.take().expect("couldnt get stderr of Youtube-DL"));

	// used to match against the parsed id (the prefix cannot be retrieved from the progress bar)
	let mut current_id: String = String::default();
	let mut current_filename: String = String::default();
	let mut current_provider: Provider = Provider::Unknown;

	bar.set_prefix(prefix_format(&current_video, &count_video, ""));

	let thread = std::thread::spawn(move || {
		// always print STDERR
		reader_stderr
			.lines()
			.filter_map(|line| return line.ok())
			.for_each(|line| {
				warn!("youtube-dl [STDERR] {}", line);
			});
	});

	// indicate if a "[download]" happened
	let mut no_download = true;

	reader_stdout.lines().filter_map(|line| return line.ok()).for_each(|line| {
		// only print STDOUT raw if debug is enabled
		if args.debug {
			trace!("youtube-dl [STDOUT] \"{}\"", line);
		}

		match LineTypes::from(line.as_ref()) {
			LineTypes::Youtube => {
				lazy_static! {
					// 1. capture group is the Video ID
					static ref YOUTUBE_MATCHER: Regex = Regex::new(r"(?mi)^\[youtube]\s*([\w\-_]*):").unwrap();
				}

				let new_id = unwrap_or_return!(YOUTUBE_MATCHER.captures_iter(&line).next())[1].to_owned();
				if current_id != new_id {
					trace!("Found new Youtube Video ID (old \"{}\", new \"{}\")", &current_id, &new_id);
					current_video += 1;
					current_id = new_id.to_owned();
					current_provider = Provider::Youtube;
					if let Some(archive) = &mut args.archive {
						// add the video to the Archive with Provider Youtube and dl_finished = false
						archive.add_video(Video::new(&current_id, Provider::Youtube));
					}
					bar.reset();
					bar.set_prefix(prefix_format(&current_video, &count_video, &new_id));
					bar.tick();
				}
			},
			LineTypes::Download => {
				lazy_static! {
					// 1. capture group is percentage
					// 2. capture group is of how much
					// 3. capture group is ETA
					// original: ^\[download]\s*(\d{1,3}.\d{1,3})%\sof\s(\d*.\d*\w{3}).*ETA\s(\d*:\d*)
					static ref DOWNLOAD_MATCHER: Regex = Regex::new(r"(?mi)^\[download]\s*(\d{1,3}).\d{1,3}%\sof\s(\d*.\d*\w{3}).*ETA\s(\d*:\d*)").unwrap();

					static ref DOWNLOAD100_MATCHER: Regex = Regex::new(r"(?mi)^\[download]\s*100%\sof\s\d*\.\d*\w*\sin\s\d*:\d*$").unwrap();

					static ref ALREADY_IN_ARCHIVE: Regex = Regex::new(r"(?mi)has already been recorded in archive").unwrap();
				}

				no_download = false;

				if let Some(filenametmp) = match_to_path(&line) {
					current_filename = filenametmp;
				}

				if DOWNLOAD100_MATCHER.is_match(&line) || ALREADY_IN_ARCHIVE.is_match(&line) {
					if ALREADY_IN_ARCHIVE.is_match(&line) {
						trace!(
							"{} Download done (Already in Archive)",
							prefix_format(&current_video, &count_video, &current_id).dimmed()
						);
						// only increase "current_video" for known providers, that do not output a id when "already being in archive"
						if current_provider == Provider::Youtube {
							current_video += 1;
						}
						bar.set_prefix(prefix_format(&current_video, &count_video, &current_id));
						bar.set_message("");
					}
					if DOWNLOAD100_MATCHER.is_match(&line) {
						bar.finish_and_clear();
						println!(
							"{} Download done \"{}\"",
							prefix_format(&current_video, &count_video, &current_id).dimmed(),
							PathBuf::from(&current_filename).file_stem().unwrap_or_else(|| return std::ffi::OsStr::new("UNKNOWN")).to_string_lossy()
						);
					}

					if let Some(archive) = &mut args.archive {
						// mark "dl_finished" for current_id if archive is used
						archive.mark_dl_finished(&current_id);
						// set the currently known filename
						archive.set_filename(&current_id, &current_filename);
					}
					return;
				}

				let position = unwrap_or_return!(DOWNLOAD_MATCHER.captures_iter(&line).next());
				bar.set_position(position[1].parse::<u64>().unwrap_or(0));
				bar.set_message("");
				bar.tick();
			},
			LineTypes::Information(provider, id, file) => {
				info!("Found PARSE Output, id: \"{}\", title: \"{}\"", &id, &file);

				if no_download {
					if let Some(archive) = &mut args.archive {
						// mark "dl_finished" for current_id (old) if archive is used, because no "already in archive" is printed when using "--print" in yt-dlp
						archive.mark_dl_finished(&current_id);
					}
				}

				no_download = true;
				current_id = id;
				current_filename = file;

				if let Some(archive) = &mut args.archive {
					archive.add_video(Video::new(&current_id, Provider::from(provider.as_str())).with_filename(&current_filename));

					return;
				}
			},
			LineTypes::Ffmpeg | LineTypes::Generic => {
				if let Some(filenametmp) = match_to_path(&line) {
					current_filename = filenametmp;

					if let Some(archive) = &mut args.archive {
						// set the currently known filename
						// (this is because ffmpeg is not always used by youtube-dl)
						archive.set_filename(&current_id, &current_filename);
					}
				}

				let ffmpeg_video: usize = if current_video < count_video && current_video > 0 {
					current_video - 1
				} else {
					current_video
				};

				bar.reset();
				bar.set_prefix(prefix_format(&ffmpeg_video, &count_video, &current_id));
				bar.set_position(50);
				bar.set_message("FFMPEG Convertion");
				bar.tick();
			},
			LineTypes::Unknown(provider) => {
				info!("line used \"YTDLOutputs::Unknown\"! (provider: \"{}\")", &provider);

				// try to capture the id, if possible
				lazy_static! {
					// 1. capture group is the Video ID
					static ref UNKNOWN_PROVIDER_ID_MATCHER: Regex = Regex::new(r"(?mi)^\[\w+]\s*([\w\-_]*):").unwrap();
				}

				let new_id = unwrap_or_return!(UNKNOWN_PROVIDER_ID_MATCHER.captures_iter(&line).next())[1].to_owned();
				if current_id != new_id {
					trace!("Found new Unknown ID (old \"{}\", new \"{}\")", &current_id, &new_id);
					current_video += 1;
					current_id = new_id.to_owned();
					current_provider = Provider::Other(provider.clone());
					if let Some(archive) = &mut args.archive {
						// add the video to the Archive with Provider::Other and dl_finished = false
						archive.add_video(Video::new(&current_id, Provider::Other(provider)));
					}
					bar.reset();
					bar.set_prefix(prefix_format(&current_video, &count_video, &new_id));
					bar.tick();
				}
			},
		}
	});

	// do it also after the lines have been processed, because "--print" will always be before the "download"
	// so it can happen that all ids are already in the archive (not printed), but the last one not already being marked as finished downloading
	// so it will be marked here
	if no_download {
		if let Some(archive) = &mut args.archive {
			// mark "dl_finished" for current_id (old) if archive is used, because no "already in archive" is printed when using "--print" in yt-dlp
			archive.mark_dl_finished(&current_id);
		}
	}

	let exit_status = spawned_ytdl
		.wait()
		.expect("Something went wrong while waiting for youtube-dl to finish... (Did it even run?)");

	thread.join().expect("Couldnt join back STDERR Handler Thread");

	if !exit_status.success() {
		match exit_status.code() {
			Some(code) => {
				return Err(ioError::new(
					ErrorKind::Other,
					format!(
						"Youtube-DL exited with a non-zero status, Stopping YT-DL-Rust (exit code: {})",
						code
					),
				))
			},
			None => {
				return Err(ioError::new(
					ErrorKind::Other,
					"Youtube-DL exited with a non-zero status, Stopping YT-DL-Rust (exit by signal?)",
				))
			},
		};
	}

	return Ok(());
}

/// check line for "Destination: " and return an option
fn match_to_path(line: &str) -> Option<String> {
	lazy_static! {
		// 1. capture group is filename
		static ref MATCH_DESTINATION: Regex = Regex::new(r"(?m)Destination:\s+(?P<filename>.+)").unwrap();
	}

	let filenametmp: &str = MATCH_DESTINATION.captures(line)?.name("filename")?.as_str();

	return Some(Path::new(filenametmp).file_name()?.to_str()?.to_string());
}
