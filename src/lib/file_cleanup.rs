use super::utils::Arguments;
use std::io::Error as ioError;

use std::fs::remove_dir_all;

pub fn file_cleanup(args: &Arguments) -> Result<(), ioError> {
	info!("Cleanup of tmp files");
	return remove_dir_all(&args.tmp);
}
