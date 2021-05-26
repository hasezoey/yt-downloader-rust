use super::archive_schema::{
	Provider,
	Video,
};
use super::spawn_multi_platform::*;
use super::utils::{
	Arguments,
	YTDLOutputs,
};

use colored::*;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use regex::Regex;
use std::fs::File;
use std::io::{
	BufRead, // is needed because otherwise ".lines" does not exist????
	BufReader,
	Error as ioError,
	ErrorKind,
	Write,
};
use std::path::{
	Path,
	PathBuf,
};
use std::process::Stdio;

lazy_static! {
	static ref YTDL_ERROR: Regex = Regex::new(r"(?mi)^ERROR").unwrap();
}

/// Count all videos in the playlist or single video
fn count(args: &Arguments) -> Result<u32, ioError> {
	let mut ytdl = spawn_command();
	ytdl.arg("-s").arg("--flat-playlist").arg("--get-id");
	ytdl.arg(&args.url);

	let mut spawned = ytdl.stdout(Stdio::piped()).spawn()?;

	let reader = BufReader::new(
		spawned
			.stdout
			.take()
			.expect("couldnt get stdout of Youtube-DL (counter)"),
	);

	let mut count: u32 = 0;

	reader.lines().filter_map(|line| return line.ok()).for_each(|_| {
		count += 1;
	});

	let exit_status = spawned.wait().expect("youtube-dl (counter) wasnt running??");

	if !exit_status.success() {
		return Err(ioError::new(
			ErrorKind::Other,
			"Youtube-DL exited with a non-zero status (Counter), Stopping YT-DL-Rust",
		));
	}

	return Ok(count);
}

lazy_static! {
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
fn prefix_format<T: AsRef<str>>(current: &u32, count: &u32, id: T) -> String {
	if id.as_ref().is_empty() {
		return format!("[{}/{}]", &current, &count);
	}

	return format!("[{}/{}] ({})", &current, &count, id.as_ref());
}

/// Spawn the main Youtube-dl task
pub fn spawn_ytdl(args: &mut Arguments) -> Result<(), ioError> {
	let count_video = count(&args)?;
	let mut current_video: u32 = 0;

	let mut ytdl = spawn_command();
	// it needs to be a string, otherwise the returns would complain about not living long enough
	let tmpdir = Path::new(&args.tmp).join("%(title)s.%(ext)s");

	if args.audio_only {
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

			for (provider, id) in archive.to_ytdl_archive() {
				writeln!(archive_handle, "{} {}", &provider, &id).expect("Couldnt Write to archive_tmp file!");
			}
		}

		ytdl.arg("--download-archive").arg(&archive_tmp);
	}

	ytdl.arg("--newline"); // to make parsing easier

	ytdl.arg("-o").arg(tmpdir);
	for arg in args.extra_args.iter() {
		ytdl.arg(arg);
	}

	ytdl.arg(&args.url);

	let mut spawned = ytdl
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()?;

	let reader_stdout = BufReader::new(spawned.stdout.take().expect("couldnt get stdout of Youtube-DL"));
	let reader_stderr = BufReader::new(spawned.stderr.take().expect("couldnt get stderr of Youtube-DL"));

	// used to match against the parsed id (the prefix cannot be retrieved from the progress bar)
	let mut current_id: String = String::default();
	let mut current_filename: String = String::default();

	let bar: ProgressBar = ProgressBar::new(100).with_style(SINGLE_STYLE.clone());

	bar.set_prefix(prefix_format(&current_video, &count_video, ""));

	let thread = std::thread::spawn(move || {
		// always print STDERR
		reader_stderr
			.lines()
			.filter_map(|line| return line.ok())
			.for_each(|line| {
				println!("[STDERR] {}", line);
			});
	});

	if args.debug {
		bar.println("Printing YTDL raw-Output");
	}

	reader_stdout.lines().filter_map(|line| return line.ok()).for_each(|line| {
		// only print STDOUT raw if debug is enabled
		if args.debug {
			bar.println(format!("[STDOUT] {}", line));
		}

		let matched = match YTDLOutputs::try_match(&line) {
			Ok(v) => v,
			Err(err) => {
				bar.println(format!("{}", err));
				return;
			},
		};

		match matched {
			YTDLOutputs::Youtube => {
				lazy_static! {
					// 1. capture group is the Video ID
					static ref YOUTUBE_MATCHER: Regex = Regex::new(r"(?mi)^\[youtube]\s*([\w\-_]*):").unwrap();
				}

				let new_id = unwrap_or_return!(YOUTUBE_MATCHER.captures_iter(&line).next())[1].to_owned();
				if current_id != new_id {
					current_video += 1;
					current_id = new_id.to_owned();
					if let Some(archive) = &mut args.archive {
						// add the video to the Archive with Provider Youtube and dl_finished = false
						archive.add_video(Video::new(&current_id, Provider::Youtube));
					}
					bar.reset();
					bar.set_prefix(prefix_format(&current_video, &count_video, &new_id));
					bar.tick();
				}
			},
			YTDLOutputs::Download => {
				lazy_static! {
					// 1. capture group is percentage
					// 2. capture group is of how much
					// 3. capture group is ETA
					// original: ^\[download]\s*(\d{1,3}.\d{1,3})%\sof\s(\d*.\d*\w{3}).*ETA\s(\d*:\d*)
					static ref DOWNLOAD_MATCHER: Regex = Regex::new(r"(?mi)^\[download]\s*(\d{1,3}).\d{1,3}%\sof\s(\d*.\d*\w{3}).*ETA\s(\d*:\d*)").unwrap();

					static ref DOWNLOAD100_MATCHER: Regex = Regex::new(r"(?mi)^\[download]\s*100%\sof\s\d*\.\d*\w*\sin\s\d*:\d*$").unwrap();

					static ref ALREADY_IN_ARCHIVE: Regex = Regex::new(r"(?mi)has already been recorded in archive").unwrap();
				}

				if let Some(filenametmp) = match_to_path(&line) {
					current_filename = filenametmp;
				}

				if DOWNLOAD100_MATCHER.is_match(&line) || ALREADY_IN_ARCHIVE.is_match(&line) {
					if ALREADY_IN_ARCHIVE.is_match(&line) {
						trace!(
							"{} Download done (Already in Archive)",
							prefix_format(&current_video, &count_video, &current_id).dimmed()
						);
						current_video += 1;
						bar.set_prefix(prefix_format(&current_video, &count_video, &current_id));
						bar.set_message("");
					}
					if DOWNLOAD100_MATCHER.is_match(&line) {
						bar.finish_and_clear();
						println!(
							"{} Download done \"{}\"",
							prefix_format(&current_video, &count_video, &current_id).dimmed(),
							PathBuf::from(&current_filename).file_stem().unwrap_or_else(|| return std::ffi::OsStr::new("UNKOWN")).to_string_lossy()
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
			YTDLOutputs::FFMPEG | YTDLOutputs::Generic => {
				if let Some(filenametmp) = match_to_path(&line) {
					current_filename = filenametmp;

					if let Some(archive) = &mut args.archive {
						// set the currently known filename
						// (this is because ffmpeg is not always used by youtube-dl)
						archive.set_filename(&current_id, &current_filename);
					}
				}

				let ffmpeg_video: u32;

				if current_video < count_video {
					ffmpeg_video = current_video - 1;
				} else {
					ffmpeg_video = current_video;
				}

				bar.reset();
				bar.set_prefix(prefix_format(&ffmpeg_video, &count_video, &current_id));
				bar.set_position(50);
				bar.set_message("FFMPEG Convertion");
				bar.tick();
			},
			_ => {},
		}
	});

	let exit_status = spawned
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

	let filenametmp: &str = MATCH_DESTINATION.captures(&line)?.name("filename")?.as_str();

	return Some(Path::new(filenametmp).file_name()?.to_str()?.to_string());
}
