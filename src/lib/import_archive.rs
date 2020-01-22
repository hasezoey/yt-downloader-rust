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
	Read,
	Seek,
	SeekFrom,
};

use super::archive_schema::{
	Archive,
	Provider,
	Video,
};
use super::setup_archive::{
	get_path,
	setup_archive,
};
use crate::unwrap_or_return;

lazy_static! {
	static ref IMPORT_STYLE: ProgressStyle = ProgressStyle::default_bar()
		.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
		.progress_chars("#>-");

	// 1. capture group is the provider
	// 2. capture group is the ID
	static ref ARCHIVE_REGEX: Regex = Regex::new(r"(?mi)^(\w+)\s+([\w\-_]+)$").unwrap();
}

pub fn import_archive(matches: &clap::ArgMatches) -> Result<Archive, ioError> {
	let input_path = get_path(&matches.value_of("input").unwrap());
	if !input_path.is_file() {
		panic!("\"{}\" is not an file!", input_path.display());
	}

	let mut archive = setup_archive(&matches.value_of("output").unwrap()).expect("Setting up the Archive failed");

	let mut reader = BufReader::new(File::open(input_path)?);

	let bar: ProgressBar = ProgressBar::new(reader.by_ref().lines().count() as u64).with_style(IMPORT_STYLE.clone());

	bar.set_position(0);

	reader.seek(SeekFrom::Start(0))?; // reset file "byte" pointer to 0
	reader
		.by_ref()
		.lines()
		.filter_map(|line| return line.ok())
		.for_each(|line| {
			bar.inc(1);
			let tmp = unwrap_or_return!(ARCHIVE_REGEX.captures_iter(&line).next());

			archive.add_video(Video::new(&tmp[2].to_owned(), Provider::try_match(&tmp[1])).set_dl_finished(true));
		});

	bar.finish_with_message("Import Finished");

	return Ok(archive);
}
