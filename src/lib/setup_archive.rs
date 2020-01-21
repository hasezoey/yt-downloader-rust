use super::archive_schema::Archive;
use super::utils::Arguments;

use std::fs::{
	create_dir_all,
	File,
};
use std::io::{
	BufReader,
	Error as ioError,
	Write,
};
use std::path::PathBuf;

fn get_path(val: &str) -> PathBuf {
	return PathBuf::from(
		val
			// trim unwanted spaces
			.trim()
			// if the path contains "~", then replace it with the home directory
			.replace("~", &std::env::var("HOME").unwrap()),
	);
}

/// Setup Archive, if correct path
/// Returns "None" if the path is invalid
pub fn setup_archive(val: &str) -> Option<Archive> {
	if val.len() == 0 {
		info!("Archive Path length is 0, working without an Archive");
		return None;
	}
	let mut path = get_path(&val);

	if path.is_dir() {
		info!("Provided Archive-Path was an directory");
		path.push("ytdl_archive");
	}

	path.set_extension("json");

	if !path.exists() {
		info!("Creating Archive File at \"{}\"", path.display());

		create_dir_all(PathBuf::from(&path).parent().unwrap())
			.expect("Recursivly creating directory(s) for Archive File Failed");

		let writer = File::create(&path).expect("Archive File Creation Error(1)");
		write_archive(&writer, &Archive::default()).expect("Archive File Creation Error(2)");

		info!("Archive File created at \"{}\"", &path.display());
		// writer gets automaticly closed by rust when exiting the scope
	}

	path = path.canonicalize().expect("Normalizing the Archive Path failed");

	debug!("Reading Archive File from \"{}\"", path.display());

	let reader = BufReader::new(File::open(&path).expect("Archive File Reading Error"));

	let mut ret: Archive =
		serde_json::from_reader(reader).expect("Something went wrong reading the Archive File into Serde");

	ret.path = path;

	return Some(ret);
}

/// if an Archive is existing in Arguments, write it
pub fn finish_archive(args: &Arguments) -> Result<(), ioError> {
	debug!("Finishing Archive");
	let archive = match &args.archive {
		Some(d) => d,
		None => {
			info!("No Archive, not writing");
			return Ok(());
		},
	};
	create_dir_all(PathBuf::from(&archive.path).parent().unwrap())
		.expect("Recursivly creating directory(s) for Archive File Failed");
	let writer = File::create(&archive.path)?;

	write_archive(&writer, &archive)?;

	return Ok(());
}

/// Write Archive pretty in debug or normal in release
fn write_archive<T>(writer: T, archive: &Archive) -> Result<(), ioError>
where
	T: Write,
{
	if cfg!(debug_assertions) {
		info!("Writing Archive PRETTY to \"{}\"", &archive.path.display());
		serde_json::to_writer_pretty(writer, &archive)?;
	} else {
		serde_json::to_writer(writer, &archive)?;
	}

	return Ok(());
}
