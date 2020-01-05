#![allow(clippy::needless_return)]
#![deny(clippy::implicit_return)]

// TODO: Progress Bar
// TODO: Split into multiple files
// TODO: cleanup

#[macro_use]
extern crate clap;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate serde;

use clap::App;
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

// type GenericErrorResult<T> = Result<T, Box<dyn Error>>;

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
}

#[derive(Debug)]
enum YTDLOutputs {
	Youtube,
	Download,
	FFMPEG,
	Unkown,
}

impl YTDLOutputs {
	pub fn try_match(input: String) -> Result<YTDLOutputs, GenericError> {
		lazy_static! {
			static ref YTDL_OUTPUT_MATCHER: Regex = Regex::new(r"(?m)^\s*\[(ffmpeg|download|\w*)\]").unwrap();
		}
		let cap = YTDL_OUTPUT_MATCHER
			.captures_iter(&input)
			.next()
			.ok_or_else(|| return GenericError::new(format!("Coudlnt parse type for \"{}\"", input)))?;

		return Ok(match &cap[1] {
			"ffmpeg" => YTDLOutputs::FFMPEG,
			"download" => YTDLOutputs::Download,
			"youtube" => YTDLOutputs::Youtube,
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

	println!("tmpdir {:?}", args.tmp);
	println!("tmp_sub {:?}", args.tmp_sub);
	println!("args: {:?}", args); // DEBUG
	spawn_ytdl(&args)?;

	println!("args2: {:?}", args); // DEBUG

	return Ok(());
}

fn spawn_ytdl(args: &Arguments) -> Result<(), ioError> {
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

	let spawned = ytdl.stdout(Stdio::piped()).spawn()?;

	let reader = BufReader::new(spawned.stdout.expect("couldnt get stdout of Youtube-DL"));

	reader.lines().filter_map(|line| return line.ok()).for_each(|line| {
		println!("{}", line);
		let matched = match YTDLOutputs::try_match(line) {
			Ok(v) => v,
			Err(err) => {
				println!("{}", err.description());
				return;
			},
		};

		println!("type: {:?}", matched);
		// TODO: Do more with "matched" (progress bar?)
	});

	return Ok(());
}
