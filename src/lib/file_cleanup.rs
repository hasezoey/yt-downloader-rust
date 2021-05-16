use super::utils::Arguments;

use std::fs::remove_dir_all;

pub fn file_cleanup(args: &Arguments) -> std::io::Result<()> {
	info!("Cleanup of tmp files");
	return remove_dir_all(&args.tmp);
}
