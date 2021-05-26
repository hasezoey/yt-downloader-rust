use super::utils::Arguments;
use fs_extra::file::{
	move_file,
	CopyOptions,
};
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use std::io::Error as ioError;
use std::path::{
	Path,
	PathBuf,
};

/// Move all files from TMP to OUT
pub fn move_finished_files(args: &Arguments) -> Result<(), ioError> {
	info!("Starting to move files");
	let out_path =
		PathBuf::from(shellexpand::tilde(&args.out.to_str().expect("Converting OUT to str failed")).as_ref());
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
		for file in std::fs::read_dir(Path::new(&args.tmp))? {
			// 2. convert Result<DirEntry> to PathBuf
			let file = file?.path();

			if file.is_dir() {
				info!("Encountered an Directory, skipping: \"{}\"", file.display());
				continue;
			};

			// 3. check if the file has an extension, when not skip it
			let ext = (match file.extension() {
				Some(v) => v,
				None => continue,
			})
			.to_str()
			.expect("Failed to convert OsStr to str")
			.to_lowercase();

			// 4. check the extension and filter
			match ext.as_ref() {
				"txt" | "jpg" | "png" => continue,
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

	for file in files {
		bar.inc(1);

		let file_name = PathBuf::from(file.file_name().expect("Couldnt get the filename"));
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
	options.overwrite = true; // for now it will always overwrite, see #3

	move_file(file, target, &options).expect("Failed to move the file to target");

	return Ok(());
}
