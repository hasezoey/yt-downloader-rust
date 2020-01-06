#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

// TODO: Progress Bar
// TODO: Split into multiple files
// TODO: cleanup

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate colored;
extern crate indicatif;
extern crate regex;
extern crate serde;

use clap::App;
use colored::*;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use regex::Regex;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;
use std::fs::create_dir_all;
use std::io::{
	BufRead, // is needed because otherwise ".lines" does not exist????
	BufReader,
	Error as ioError,
	ErrorKind,
};
use std::path::Path;
use std::process::{
	Command,
	Stdio,
};
use std::str;

#[derive(Debug)]
struct GenericError {
	details: String,
}

impl GenericError {
	pub fn new<S: Into<String>>(msg: S) -> GenericError {
		return GenericError { details: msg.into() };
	}
}

impl fmt::Display for GenericError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		return write!(f, "{}", self.details);
	}
}

impl Error for GenericError {
	fn description(&self) -> &str {
		return &self.details;
	}
}

#[derive(Debug)]
/// Arguments for Youtube-DL
struct Arguments {
	/// Output directory
	pub out:        String,
	/// Temporary Directory
	pub tmp:        String,
	/// Create a Sub-Directory in the Temporary Directory?
	pub tmp_sub:    String,
	/// The URL to download
	pub url:        String,
	/// Extra options passed to youtube-dl
	pub extra_args: Vec<String>,
	/// Audio Only?
	pub audio_only: bool,
	/// print youtube-dl stdout?
	pub debug:      bool,
}

#[derive(Debug)]
enum YTDLOutputs {
	Youtube,
	Download,
	FFMPEG,
	Generic,
	Unkown,
}

impl YTDLOutputs {
	pub fn try_match(input: &String) -> Result<YTDLOutputs, GenericError> {
		lazy_static! {
			static ref YTDL_OUTPUT_MATCHER: Regex = Regex::new(r"(?mi)^\s*\[(ffmpeg|download|[\w:]*)\]").unwrap();
			static ref YTDL_OUTPUT_GENERIC: Regex = Regex::new(r"(?mi)^\s*Deleting\soriginal").unwrap();
		}

		if YTDL_OUTPUT_GENERIC.is_match(input) {
			return Ok(YTDLOutputs::Generic);
		}

		let cap = YTDL_OUTPUT_MATCHER
			.captures_iter(input)
			.next()
			.ok_or_else(|| return GenericError::new(format!("Coudlnt parse type for \"{}\"", input)))?;

		return Ok(match &cap[1] {
			"ffmpeg" => YTDLOutputs::FFMPEG,
			"download" => YTDLOutputs::Download,
			"youtube" => YTDLOutputs::Youtube,
			"youtube:playlist" => YTDLOutputs::Youtube,
			_ => {
				println!("unkown: {:?}", &cap[1]);
				YTDLOutputs::Unkown
			},
		});
	}
}

// Main
fn main() -> Result<(), ioError> {
	let yml = load_yaml!("cli.yml");
	let cli_matches = App::from_yaml(yml).get_matches();
	let mut args = Arguments {
		out:        cli_matches.value_of("out").unwrap().to_owned(), // unwrap, because of a set default
		tmp:        cli_matches.value_of("tmp").unwrap().to_owned(), // unwrap, because of a set default
		url:        cli_matches.value_of("URL").unwrap().to_owned(), // unwrap, because "URL" is required
		tmp_sub:    cli_matches.value_of("tmpcreate").unwrap().to_owned(), // unwrap, because of a set default
		audio_only: cli_matches.is_present("audio_only"),
		debug:      cli_matches.is_present("debug"),
		extra_args: cli_matches
			.values_of("ytdlargs") // get all values after "--"
			.map(|v| return v.collect::<Vec<&str>>()) // because "clap::Values" is an iterator, collect it all as Vec<&str>
			.unwrap_or(Vec::new()) // unwrap the Option<Vec<&str>> or create a bew Vec
			.iter() // Convert the Vec<&str> to an iterator
			.map(|v| return String::from(*v)) // Map every value to String (de-referencing because otherwise it would be "&&str")
			.collect(), // Collect it again as Vec<String>
	};

	args.extra_args.push("--write-thumbnail".to_owned());
	args.tmp = match args.tmp_sub.as_ref() {
		"true" => {
			let lepath = Path::new(&args.tmp).join("rust-yt-dl");

			create_dir_all(&lepath).expect("Couldnt create tmpsub directory");

			lepath
				.canonicalize()
				.expect("failed to canonicalize a path")
				.to_str()
				.expect("failed to parse a PathBuf to a string")
				.to_owned()
		},
		"false" => Path::new(&args.tmp)
			.canonicalize()
			.expect("failed to canonicalize a path")
			.to_str()
			.expect("failed to parse a PathBuf to a string")
			.to_owned(),
		_ => return Err(ioError::new(ErrorKind::Other, "Invalid tmpcreate value!")),
	};

	// println!("tmpdir {:?}", args.tmp);
	// println!("tmp_sub {:?}", args.tmp_sub);
	// println!("args: {:?}", args); // DEBUG
	spawn_ytdl(&args)?;

	// println!("args2: {:?}", args); // DEBUG

	return Ok(());
}

/// Count all videos in the playlist or single video
fn count(args: &Arguments) -> Result<u32, ioError> {
	let mut ytdl = Command::new("youtube-dl");
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

fn spawn_ytdl(args: &Arguments) -> Result<(), ioError> {
	let count_video = count(&args)?;
	let mut current_video: u32 = 0;

	let mut ytdl = Command::new("youtube-dl");
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
