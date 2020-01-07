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
use std::error::Error; // needed, otherwise "error.description" cannot be used
use std::io::{
	BufRead, // is needed because otherwise ".lines" does not exist????
	BufReader,
	Error as ioError,
};
use std::path::Path;
use std::process::Stdio;

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

	spawned.wait().expect("youtube-dl (counter) wasnt running??");

	return Ok(count);
}

lazy_static! {
	static ref SINGLE_STYLE: ProgressStyle = ProgressStyle::default_bar()
		.template("{prefix:.dim} [{elapsed_precise}] {bar:40.cyan/blue} {msg}")
		.progress_chars("#>-");
}

/// shorthand for unwrapping or early-returning
macro_rules! unwrap_or_return {
	($e:expr) => {
		match $e {
			Some(v) => v,
			None => return,
			}
	};
}

/// to have a unified prefix
macro_rules! prefix_format {
	($cur:expr, $cou:expr, $id:expr) => {
		format!("[{}/{}] ({})", $cur, $cou, $id)
	};
}

/// Spawn the main Youtube-dl task
pub fn spawn_ytdl(args: &Arguments) -> Result<(), ioError> {
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

	ytdl.arg("--newline"); // to make parsing easier

	ytdl.arg("-o").arg(tmpdir);
	for arg in args.extra_args.iter() {
		ytdl.arg(arg);
	}

	ytdl.arg(&args.url);

	let mut spawned = ytdl.stdout(Stdio::piped()).spawn()?;

	let reader = BufReader::new(spawned.stdout.take().expect("couldnt get stdout of Youtube-DL"));

	// used to match against the parsed id (the prefix cannot be retrieved from the progress bar)
	let mut current_id: String = String::from("");

	let bar: ProgressBar = ProgressBar::new(100).with_style(SINGLE_STYLE.clone());

	bar.set_prefix(&prefix_format!(current_video, count_video, "<none>"));
	bar.set_position(0);

	reader.lines().filter_map(|line| return line.ok()).for_each(|line| {
		if args.debug {
			bar.println(format!("{}", line));
		}

		let matched = match YTDLOutputs::try_match(&line) {
			Ok(v) => v,
			Err(err) => {
				bar.println(format!("{}", err.description()));
				return;
			},
		};

		match matched {
			YTDLOutputs::Youtube => {
				lazy_static! {
					// 1. capture group is the Video ID
					static ref YOUTUBE_MATCHER: Regex = Regex::new(r"(?mi)^\[youtube]\s*([\w-]*):").unwrap();
				}

				let tmp = unwrap_or_return!(YOUTUBE_MATCHER.captures_iter(&line).next())[1].to_owned();
				if current_id != tmp {
					current_video += 1;
					current_id = tmp.to_owned();
					bar.reset();
					bar.set_prefix(&prefix_format!(current_video, count_video, &tmp));
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
				}

				if DOWNLOAD100_MATCHER.is_match(&line) {
					bar.finish_and_clear();
					println!("{}", format!(
						"{} Download done",
						prefix_format!(current_video, count_video, current_id).dimmed()
					));
					return;
				}

				let tmp = unwrap_or_return!(DOWNLOAD_MATCHER.captures_iter(&line).next());
				bar.set_position(tmp[1].parse::<u64>().unwrap_or(0));
				bar.set_message(&format!(""));
			},
			YTDLOutputs::FFMPEG | YTDLOutputs::Generic => {
				bar.reset();
				bar.set_position(99);
				bar.set_message("FFMPEG Convertion");
			},
			_ => {},
		}
	});

	spawned
		.wait()
		.expect("Something went wrong while waiting for youtube-dl to finish... (Did it even run?)");

	return Ok(());
}
