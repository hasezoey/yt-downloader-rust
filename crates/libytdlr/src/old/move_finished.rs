use crate::old::utils::ResponseYesNo;
use fs_extra::file::{
	move_file,
	CopyOptions,
};
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use std::io::{
	Error as ioError,
	Write,
};
use std::path::{
	Path,
	PathBuf,
};

/// Move all files from TMP to OUT
pub fn move_finished_files<O: AsRef<Path>, T: AsRef<Path>>(
	out_path: O,
	tmp_path: T,
	debug: bool,
) -> Result<(), ioError> {
	info!("Starting to move files");
	let out_path = crate::utils::expand_tidle(out_path.as_ref())
		.ok_or_else(|| return ioError::new(std::io::ErrorKind::InvalidInput, "Failed to Expand \"~\""))?;

	std::fs::create_dir_all(&out_path)
		.or_else(|err| {
			if let Some(raw_os_error) = err.raw_os_error() {
				if raw_os_error == 17 {
					trace!("create_dir_all failed, because path already exists");
					return Ok(());
				}
			}

			return Err(err);
		})
		.expect("Creating the OUT directory failed!");

	let files: Vec<PathBuf> = {
		// Convert "read_dir" to useable files, Steps:
		let mut tmp: Vec<PathBuf> = Vec::default();
		// 1. read the dir
		for file in std::fs::read_dir(tmp_path.as_ref())? {
			// 2. convert Result<DirEntry> to PathBuf
			let file = file?.path();

			if file.is_dir() {
				info!("Encountered an Directory, skipping: \"{}\"", file.display());
				continue;
			};

			// 3. check if the file has an extension, when not skip it
			let ext = (match file.extension() {
				Some(v) => v,
				// skip files that dont have an file extension
				None => continue,
			})
			.to_str()
			.expect("Failed to convert OsStr to str")
			.to_lowercase();

			// 4. check the extension and filter
			match ext.as_ref() {
				// skip files that have one of the following extensions
				"txt" | "jpg" | "png" | "webp" | "json" | "yml" => continue,
				_ => (),
			}

			// 5. push PathBuf to the returning Vector
			tmp.push(file);
		}
		tmp // return from block
	};

	// Early return in case nothing is found to save extra executing time
	if files.is_empty() {
		return Ok(());
	}

	let bar: ProgressBar = ProgressBar::new(files.len() as u64).with_style(
		ProgressStyle::default_bar()
			.template("[{pos}/{len}] [{elapsed_precise}] {bar:40.cyan/blue} {msg}")
			.progress_chars("#>-"),
	);

	if debug {
		bar.set_draw_target(indicatif::ProgressDrawTarget::hidden())
	}

	for file in files {
		bar.inc(1);

		let file_name = PathBuf::from(file.file_name().expect("Couldnt get the filename"));
		// skip files that either have no extension, or are one of the specified
		let target = Path::new(&out_path).join(&file_name);

		mv_handler(&file, &target)?;
	}

	bar.finish_with_message("Moving Files, Done");
	info!("Moving Files from TMP to OUT finished");

	return Ok(());
}

/// Move files from "file" to "target" with logging
pub fn mv_handler(file: &Path, target: &Path) -> Result<(), ioError> {
	info!("Moving file from \"{}\" to \"{}\"\n", file.display(), target.display());
	let mut options = CopyOptions::new();

	if target.exists() {
		match ask_overwrite(target) {
			Ok(answer) => match answer {
				ResponseYesNo::Yes => options.overwrite = true, // for now it will always overwrite, see #3
				ResponseYesNo::No => return Ok(()),             // return "OK" to continue program flow
			},
			Err(err) => return Err(err),
		}
	}

	move_file(file, target, &options).expect("Failed to move the file to target");

	return Ok(());
}

/// Repeat to ask Yes or No until valid
fn ask_overwrite(file: &Path) -> Result<ResponseYesNo, ioError> {
	println!("Do you want to overwrite \"{}\"?", file.to_string_lossy());
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
