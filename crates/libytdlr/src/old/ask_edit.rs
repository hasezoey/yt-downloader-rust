use super::move_finished::mv_handler;
use super::utils::Arguments;
use crate::data::video::Video;
use crate::old::utils::{
	ResponseContinue,
	ResponseYesNo,
};
use std::fs::metadata;
use std::io::ErrorKind;
use std::io::{
	Error as ioError,
	Write,
};
use std::path::{
	Path,
	PathBuf,
};
use std::process::Stdio;
use std::process::{
	Child,
	ExitStatus,
};

/// Ask for edits on donwloaded files
pub fn edits(args: &mut Arguments) -> Result<(), ioError> {
	let archive = args.archive.as_mut().unwrap(); // unwrap because it is checked before
	debug!("Asking for Edit");
	if args.editor.is_empty() {
		println!("Please enter an command to be used as editor, or leave it empty to skip it");
		print!("$ ");
		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		args.editor = input.trim().to_owned();
		debug!("Editor entered: \"{}\"", args.editor);

		if args.editor.is_empty() {
			// if it is still empty, just dont ask for edits
			info!("Editor is empty, not asking for edits");
			return Ok(());
		}
	}

	if archive.get_videos().is_empty() {
		debug!("Archive Videos is empty!");

		return Ok(());
	}

	debug!("Starting Edit ask loop");
	let mut edited: Vec<PathBuf> = Vec::new();
	// TODO: Reformat (get_mut_videos) to use iterators
	for video in archive.get_videos_mut() {
		if video.edit_asked() {
			trace!("Video \"{}\" has already been asked to edit", video.id());

			continue;
		}

		// Skip the video if the filename is empty
		if video.file_name().is_empty() {
			info!("{} does not have an filename!", video);
			video.set_edit_asked(true);
			continue;
		}

		if ask_edit(video)? == ResponseYesNo::No {
			video.set_edit_asked(true);
			continue;
		}

		let video_path = Path::new(&args.tmp).join(&video.file_name());

		// test if the video file can even still be found in the tmp directory
		if let Err(err) = metadata(&video_path) {
			info!("Video not found in tmp directory! Error:\n{}", err);
			video.set_edit_asked(true); // set asked to true, even though not asked - the video cant be found in the temporary directory anymore
			continue;
		}

		// a loop to make it easier to re-try if the editor somehow crashed
		loop {
			match spawn_editor(&args.editor, &video_path, args.debug) {
				Ok(exit_status) => {
					// early return for performance
					if exit_status.success() {
						break;
					}

					warn!(
						"The Editor Failed with a non-zero exist code! (code: \"{}\")",
						exit_status
					);
					match ask_continue(video)? {
						// continue loop (re-try spawning editor)
						ResponseContinue::Retry => continue,
						// abort loop and return a (graceful) error, with proper tmp archive writing
						ResponseContinue::Abort => {
							return Err(ioError::new(
								ErrorKind::Other,
								"The Editor exited with a non-zero status, Stopping YT-DL-Rust",
							))
						},
						// handle as if normal exit (no retry)
						ResponseContinue::Continue => break,
					}
				},
				// unrecoverable error happend (like not being able to spawn process), dont ask user because it can not be easily recovered from
				Err(err) => return Err(err),
			}
		}

		video.set_edit_asked(true);

		if !args.disable_re_thumbnail {
			edited.push(video_path);
		}
	}

	for video_path in edited {
		// this is needed, otherwise "&args" would be borrowed mutable and immutable
		re_thumbnail(args, &video_path)?;
	}

	return Ok(());
}

fn spawn_editor(editor: &str, filepath: &Path, debug: bool) -> Result<ExitStatus, ioError> {
	let mut editorcommand = crate::spawn::editor::base_editor(editor, filepath);

	let mut spawned_editor: Child = if debug {
		editorcommand
			.stderr(Stdio::inherit())
			.stdout(Stdio::inherit())
			.stdin(Stdio::null())
			.spawn()?
	} else {
		editorcommand
			.stderr(Stdio::null())
			.stdout(Stdio::null())
			.stdin(Stdio::null())
			.spawn()?
	};

	return Ok(spawned_editor
		.wait()
		.expect("Something went wrong while waiting for the Editor to finish... (Did it even run?)"));
}

/// Reapply the thumbnail after the video has been edited
/// Reason for this is that some editor like audacity dosnt copy the thumbnail when saving
fn re_thumbnail(_args: &Arguments, video_path: &Path) -> Result<(), ioError> {
	use crate::main::rethumbnail::*;
	info!("Reapplying thumbnail for \"{}\"", &video_path.display());
	let mut thumbnail_path = PathBuf::from(&video_path.as_os_str());
	thumbnail_path.set_extension("jpg");
	let mut ffmpegout_path = PathBuf::from(&video_path.as_os_str());
	ffmpegout_path.set_file_name(format!(
		"{}_re-apply.mp3",
		&video_path
			.file_stem()
			.expect("Expected video_path to have file_name")
			.to_str()
			.unwrap()
	));

	if let Err(err) = metadata(&thumbnail_path) {
		warn!(
			"Couldnt find \"{}\" in the Temporary directory. Error:\n{}",
			&thumbnail_path.display(),
			err
		);

		return Ok(()); // dont error out, just warn
	}

	re_thumbnail(video_path, thumbnail_path, &ffmpegout_path)?;

	mv_handler(&ffmpegout_path, video_path)?;

	info!("Finished Reapplying for \"{}\"", &video_path.display());

	return Ok(());
}

/// Repeat to ask Yes or No until valid
fn ask_edit(video: &Video) -> Result<ResponseYesNo, ioError> {
	println!("Do you want to edit \"{}\"?", video.file_name());
	loop {
		print!("[Y/n]: ");

		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		let input = input.trim().to_lowercase();

		match input.as_ref() {
			"y" | "" | "yes" => return Ok(ResponseYesNo::Yes),
			"n" | "no" => return Ok(ResponseYesNo::No),
			_ => {
				println!("Wrong Character, please use either Y or N");
				continue;
			},
		}
	}
}

/// Ask if a action should be retried or just continue or full abort
fn ask_continue(video: &Video) -> Result<ResponseContinue, ioError> {
	println!(
		"Do you want to [R]etry or [C]ontinue or [A]bort \"{}\"?",
		video.file_name()
	);
	loop {
		print!("[R/c/a]: ");

		std::io::stdout().flush()?; // ensure the print is printed
		let mut input = String::new();
		std::io::stdin().read_line(&mut input)?;
		let input = input.trim().to_lowercase();

		match input.as_ref() {
			"r" | "retry" => return Ok(ResponseContinue::Retry),
			"c" | "continue" => return Ok(ResponseContinue::Continue),
			"a" | "abort" => return Ok(ResponseContinue::Abort),
			_ => {
				println!("Wrong Character, please use R or C or A");
				continue;
			},
		}
	}
}
