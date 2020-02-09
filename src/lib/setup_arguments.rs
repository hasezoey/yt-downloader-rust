use super::setup_archive::setup_archive;
use super::utils::Arguments;

use std::fs::create_dir_all;
use std::io::{
	Error as ioError,
	ErrorKind,
};
use std::path::{
	Path,
	PathBuf,
};

fn string_to_bool(input: &str) -> bool {
	return match input {
		"true" => true,
		_ => false,
	};
}

/// Setup clap-arguments
pub fn setup_args(cli_matches: &clap::ArgMatches) -> Result<Arguments, ioError> {
	let mut args = Arguments {
		out:             PathBuf::from(&cli_matches.value_of("out").unwrap()), // unwrap, because of a set default
		tmp:             PathBuf::from(&cli_matches.value_of("tmp").unwrap()), // unwrap, because of a set default
		url:             cli_matches.value_of("URL").unwrap_or("").to_owned(), // unwrap, because "URL" is required
		tmp_sub:         cli_matches.value_of("tmpcreate").unwrap().to_owned(), // unwrap, because of a set default
		archive:         setup_archive(&cli_matches.value_of("archive").unwrap()), // unwrap, because of a set default
		audio_only:      cli_matches.is_present("audio_only"),
		debug:           cli_matches.is_present("debug"),
		disable_cleanup: cli_matches.is_present("disablecleanup"),
		askedit:         string_to_bool(cli_matches.value_of("askedit").unwrap()),
		editor:          cli_matches.value_of("editor").unwrap().to_owned(),
		extra_args:      cli_matches
			.values_of("ytdlargs") // get all values after "--"
			.map(|v| return v.collect::<Vec<&str>>()) // because "clap::Values" is an iterator, collect it all as Vec<&str>
			.unwrap_or(Vec::new()) // unwrap the Option<Vec<&str>> or create a bew Vec
			.iter() // Convert the Vec<&str> to an iterator
			.map(|v| return String::from(*v)) // Map every value to String (de-referencing because otherwise it would be "&&str")
			.collect(), // Collect it again as Vec<String>
	};

	if args.url.len() <= 0 {
		println!("URL is required!");
		std::process::exit(2);
	}

	args.extra_args.push("--write-thumbnail".to_owned());
	args.tmp = match args.tmp_sub.as_ref() {
		"true" => {
			let lepath = Path::new(&args.tmp).join("rust-yt-dl");

			create_dir_all(&lepath).expect("Couldnt create tmpsub directory");

			lepath.canonicalize().expect("failed to canonicalize a path")
		},
		"false" => Path::new(&args.tmp)
			.canonicalize()
			.expect("failed to canonicalize a path"),
		_ => return Err(ioError::new(ErrorKind::Other, "Invalid tmpcreate value!")),
	};

	return Ok(args);
}
