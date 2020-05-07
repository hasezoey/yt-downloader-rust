use super::setup_archive::get_path;
use super::utils::Arguments;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use std::io::{
	Error as ioError,
	ErrorKind,
};
use std::path::{
	Path,
	PathBuf,
};
use std::process::Command;
use std::process::Stdio;

pub fn move_finished_files(args: &Arguments) -> Result<(), ioError> {
	info!("Starting to move files");
	let out_path = get_path(&args.out.to_str().expect("Converting OUT to str failed"));
	std::fs::create_dir_all(&out_path).expect("Creating the OUT directory failed!");

	let files: Vec<PathBuf> = {
		// Convert "read_dir" to useable files, Steps:
		let mut tmp: Vec<PathBuf> = Vec::default();
		// 1. read the dir
		for file in std::fs::read_dir(Path::new(&args.tmp))? {
			// 2. convert Result<DirEntry> to PathBuf
			let file = file?.path();
			// 3. check if the file has an extension, when not skip it
			let ext = (match &file.extension() {
				Some(v) => *v,
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

	lazy_static! {
		static ref SINGLE_STYLE: ProgressStyle = ProgressStyle::default_bar()
			.template("[{pos}/{len}] [{elapsed_precise}] {bar:40.cyan/blue} {msg}")
			.progress_chars("#>-");
	}
	let bar: ProgressBar = ProgressBar::new(files.len() as u64).with_style(SINGLE_STYLE.clone());

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

pub fn mv_handler(file: &PathBuf, target: &PathBuf) -> Result<(), ioError> {
	info!(
		"Moving file from \"{}\" to \"{}\"\n",
		&file.display(),
		&target.display()
	);

	// block for the "mv" command
	{
		let mut mvcommand = Command::new("mv");
		mvcommand.arg(&file);
		mvcommand.arg(&target);

		let mut spawned = mvcommand.stdout(Stdio::piped()).spawn()?;

		let exit_status = spawned
			.wait()
			.expect("Something went wrong while waiting for \"mv\" to finish... (Did it even run?)");

		if !exit_status.success() {
			return Err(ioError::new(
				ErrorKind::Other,
				"\"mv\" exited with a non-zero status, Stopping YT-DL-Rust",
			));
		}
	}

	return Ok(());
}
