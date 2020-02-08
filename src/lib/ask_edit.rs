use super::archive_schema::Video;
use super::utils::Arguments;
use std::fs::metadata;
use std::io::{
	BufRead, // is needed because otherwise ".lines" does not exist????
	BufReader,
	ErrorKind,
};
use std::io::{
	Error as ioError,
	Write,
};
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

fn trim_newline(s: &mut String) {
	if s.ends_with('\n') {
		s.pop();
		if s.ends_with('\r') {
			s.pop();
		}
	}
}

#[derive(PartialEq)]
enum YesNo {
	Yes,
	No,
}

/// Ask for edits on donwloaded files
pub fn edits(args: &mut Arguments) -> Result<(), ioError> {
	let archive = args.archive.as_mut().unwrap(); // unwrap because it is checked before
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

		// Skip the video if the filename is empty
		if video.file_name.len() <= 0 {
			info!("{} does not have an filename!", video);
			&video.set_edit_asked(true);
			continue;
		}

		if ask_edit(&video)? == YesNo::No {
			&video.set_edit_asked(true);
			continue;
		}

		let video_path = Path::new(&args.tmp).join(&video.file_name);

		// test if the video file can even still be found in the tmp directory
		if let Err(err) = metadata(&video_path) {
			info!("Video not found in tmp directory! Error:\n{}", err);
			&video.set_edit_asked(true); // set asked to true, even though not asked - the video cant be found in the temporary directory anymore
			continue;
		}

		let mut editorcommand = Command::new(&args.editor);
		editorcommand.arg(&video_path);

		let mut spawned = editorcommand.stdout(Stdio::piped()).spawn()?;

		let reader = BufReader::new(spawned.stdout.take().expect("couldnt get stdout of the Editor"));

		if args.debug {
			// i dont know why this dosnt work in the "for_each" loop
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

	return Ok(());
}

/// Repeat to ask Yes or No until valid
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
			"y" | "" | "yes" => return Ok(YesNo::Yes),
			"n" | "no" => return Ok(YesNo::No),
			_ => {
				println!("Wrong Character, please use either Y or N");
				continue;
			},
		}
	}
}
