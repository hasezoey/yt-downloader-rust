use super::setup_archive::setup_archive;
use super::utils::Arguments;

use std::fs::create_dir_all;
use std::io::Error as ioError;
use std::path::PathBuf;

fn string_to_bool(input: &str) -> bool {
	return matches!(input, "true");
}

/// Setup clap-arguments
pub fn setup_args(cli_matches: &clap::ArgMatches) -> Result<Arguments, ioError> {
	let mut args = Arguments {
		out:             PathBuf::from(&cli_matches.value_of("out").unwrap()), // unwrap, because of a set default
		tmp:             PathBuf::from(&cli_matches.value_of("tmp").unwrap()), // unwrap, because of a set default
		url:             cli_matches.value_of("URL").unwrap_or("").to_owned(), // unwrap, because "URL" is required
		archive:         setup_archive(&cli_matches.value_of("archive").unwrap()), // unwrap, because of a set default
		audio_only:      cli_matches.is_present("audio_only"),
		debug:           cli_matches.is_present("debug"),
		disable_cleanup: cli_matches.is_present("disablecleanup"),
		d_e_thumbnail:   cli_matches.is_present("disableeditorthumbnail"),
		askedit:         string_to_bool(cli_matches.value_of("askedit").unwrap()),
		editor:          cli_matches.value_of("editor").unwrap().to_owned(),
		extra_args:      cli_matches
			.values_of("ytdlargs") // get all values after "--"
			.map(|v| return v.collect::<Vec<&str>>()) // because "clap::Values" is an iterator, collect it all as Vec<&str>
			.unwrap_or_default() // unwrap the Option<Vec<&str>> or create a new Vec
			.iter() // Convert the Vec<&str> to an iterator
			.map(|v| return String::from(*v)) // Map every value to String (de-referencing because otherwise it would be "&&str")
			.collect(), // Collect it again as Vec<String>
	};

	if args.url.is_empty() {
		println!("URL is required!");
		std::process::exit(2);
	}

	args.extra_args.push("--write-thumbnail".to_owned());

	args.tmp = args.tmp.canonicalize()?;

	// its "3" because "/" is an ancestor and "tmp" is an ancestor
	if args.tmp.ancestors().count() < 3 {
		debug!(
			"Adding another directory to YTDL_TMP, original: \"{}\"",
			args.tmp.display()
		);
		args.tmp = args.tmp.join("ytdl-rust");

		create_dir_all(&args.tmp)?;
	}

	return Ok(args);
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_arguments_tmp_add_ancestor() {
		let args = vec!["bin", "--tmp", "/tmp", "SomeURL"];
		let yml = clap::load_yaml!("../cli.yml");
		let cli_matches = clap::App::from_yaml(yml).get_matches_from(args);

		let archive = setup_args(&cli_matches).unwrap();

		assert_eq!(PathBuf::from("/tmp/ytdl-rust"), archive.tmp);
	}
}
