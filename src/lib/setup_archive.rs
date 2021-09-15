use crate::lib::paths::to_absolute;

use super::archive_schema::Archive;

use std::fs::{
	create_dir_all,
	File,
};
use std::io::{
	BufReader,
	Error as ioError,
	Write,
};
use std::path::{
	Path,
	PathBuf,
};

/// Setup Archive, if correct path
/// Returns "None" if the path is invalid
pub fn setup_archive<T: AsRef<Path>>(val: T) -> Option<Archive> {
	let input = val.as_ref();
	if input.as_os_str().is_empty() {
		debug!("Archive Path length is 0, working without an Archive");
		return None;
	}

	let mut path = to_absolute(std::env::current_dir().ok()?.as_path(), input).ok()?;

	if path.is_dir() {
		debug!("Provided Archive-Path was an directory");
		path.push("ytdl_archive");
	}

	if !(path.exists() && path.is_file()) {
		debug!("Archive Path did not exist, adding file extension");
		path.set_extension("json");
	} else {
		debug!("Archive Path already exists and is an file, not adding file extension");
	}

	if !path.exists() {
		debug!("Creating Default Archive File at \"{}\"", path.display());

		let mut default_archive = Archive::default();
		default_archive.path = path;

		write_archive(&default_archive).expect("Failed to write Archive to File");

		return Some(default_archive);
	}

	debug!("Reading Archive File from \"{}\"", path.display());

	let reader = BufReader::new(File::open(&path).expect("Archive File Reading Error"));

	let mut ret: Archive =
		serde_json::from_reader(reader).expect("Something went wrong reading the Archive File into Serde");

	ret.path = path;

	return Some(ret);
}

/// if an Archive is existing in Arguments, write it
pub fn write_archive(archive: &Archive) -> Result<(), ioError> {
	if archive.path.as_os_str().is_empty() {
		debug!("Not writing Archive, because no path got provided");
		return Ok(());
	}
	debug!("Writing Archive to File at \"{}\"", archive.path.display());
	create_dir_all(PathBuf::from(&archive.path).parent().unwrap())
		.expect("Recursivly creating directory(s) for Archive File Failed");
	let writer = File::create(&archive.path)?;

	write_archive_to_writer(&writer, archive)?;

	return Ok(());
}

/// Write Archive pretty in debug and normal in release
fn write_archive_to_writer<T>(writer: T, archive: &Archive) -> Result<(), ioError>
where
	T: Write,
{
	if cfg!(debug_assertions) {
		debug!("Writing Archive PRETTY to \"{}\"", &archive.path.display());
		serde_json::to_writer_pretty(writer, &archive)?;
	} else {
		serde_json::to_writer(writer, &archive)?;
	}

	return Ok(());
}
