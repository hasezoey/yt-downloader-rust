#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate clap;
extern crate colored;
extern crate env_logger;
extern crate indicatif;
extern crate regex;
extern crate semver;
extern crate serde;
extern crate serde_json;

use clap::load_yaml;
use clap::App;
use env_logger::{
	builder,
	Target,
};
use std::io::{
	Error as ioError,
	Write,
};

mod lib;

use lib::*;

use archive_schema::{
	Archive,
	Video,
};
use std::io::{
	BufRead, // is needed because otherwise ".lines" does not exist????
	BufReader,
	ErrorKind,
};
use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use utils::Arguments;

// TODO: implement moving files & edited files to OUT

fn trim_newline(s: &mut String) {
	if s.ends_with('\n') {
		s.pop();
		if s.ends_with('\r') {
			s.pop();
		}
	}
}

/// Main
fn main() -> Result<(), ioError> {
	builder().target(Target::Stderr).init();

	let yml = load_yaml!("./cli.yml");
	let cli_matches = App::from_yaml(yml).get_matches();

	// handle importing native youtube-dl archives
	if let Some(matches) = cli_matches.subcommand_matches("import") {
		let archive = import_archive::import_archive(&matches)?;

		setup_archive::finish_archive(&archive)?;

		return Ok(());
	}

	// handle command without subcommands (actually downloading)

	// mutable because it is needed for the archive
	let mut args = setup_arguments::setup_args(&cli_matches)?;
	let mut errcode = false;

	spawn_main::spawn_ytdl(&mut args).unwrap_or_else(|err| {
		println!("An Error Occured in spawn_ytdl (still saving archive):\n\t{}", err);
		errcode = true;
	});

	if !errcode && args.askedit {
		if let Some(_) = &args.archive {
			edits(&mut args).unwrap_or_else(|err| {
				println!("An Error Occured in edits:\n\t{}", err);
			});
		} else {
			info!("No Archive, not asking for edits");
		}
	}

	if let Some(archive) = &args.archive {
		setup_archive::finish_archive(&archive)?;
	} else {
		info!("No Archive, not writing");
	}

	// if an error happened, exit with an non-zero error code
	if errcode {
		warn!("Existing with non-zero code, because of an previous Error");
		std::process::exit(1);
	}
	return Ok(());
}

#[derive(PartialEq)]
enum YesNo {
	Yes,
	No,
}

/// Ask for edits on donwloaded files
fn edits(args: &mut Arguments) -> Result<(), ioError> {
	let mut archive = args.archive.as_mut().unwrap(); // unwrap because it is checked before
	debug!("Asking for Edit");
	if args.editor.len() <= 0 {
		println!("Please enter an command to be used as editor, or leave it empty to skip it");
		print!("$ ");
		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		trim_newline(&mut input); // trim the newline at the end
		args.editor = input.trim().to_owned();
		debug!("Editor entered: {}", args.editor);

		if args.editor.len() <= 0 {
			// if it is still empty, just dont ask for edits
			info!("Editor is empty, not asking for edits");
			return Ok(());
		}
	}
	debug!("Starting Edit ask loop");
	for video in &mut archive.get_mut_videos().iter_mut() {
		if video.edit_asked {
			continue;
		}

		if video.file_name.len() <= 0 {
			info!("{} does not have an filename!", video);
			continue;
		}

		if ask_edit(&video)? == YesNo::No {
			continue;
		}

		let mut editorcommand = Command::new(&args.editor);
		editorcommand.arg(Path::new(&args.tmp).join(&video.file_name));

		let mut spawned = editorcommand.stdout(Stdio::piped()).spawn()?;

		let reader = BufReader::new(spawned.stdout.take().expect("couldnt get stdout of the Editor"));

		if args.debug {
			reader.lines().filter_map(|line| return line.ok()).for_each(|line| {
				println!("Editor Output: {}", line);
			});
		}

		let exit_status = spawned
			.wait()
			.expect("Something went wrong while waiting for the Editor to finish... (Did it even run?)");

		if !exit_status.success() {
			return Err(ioError::new(
				ErrorKind::Other,
				"The Editor exited with a non-zero status, Stopping YT-DL-Rust",
			));
		}

		&video.set_edit_asked(true);
	}
	// TODO: Ask for Edit

	return Ok(());
}

fn ask_edit(video: &Video) -> Result<YesNo, ioError> {
	println!("Do you want to edit \"{}\"?", video.file_name);
	loop {
		print!("[Y/n]: ");

		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		trim_newline(&mut input); // trim the newline at the end
		let input = input.trim().to_lowercase();

		match input.as_ref() {
			"y" | "" => return Ok(YesNo::Yes),
			"n" => return Ok(YesNo::No),
			_ => {
				println!("Wrong Character, please use either Y or N");
				continue;
			},
		}
	}
}
