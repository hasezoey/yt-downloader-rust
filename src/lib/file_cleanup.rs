use super::utils::Arguments;
use std::io::{
	Error as ioError,
	ErrorKind,
};

use std::process::Command;
use std::process::Stdio;

pub fn file_cleanup(args: &Arguments) -> Result<(), ioError> {
	info!("Cleanup of tmp files");
	// block for the "rm" command
	let mut rmcommand = Command::new("rm");
	rmcommand.arg("-rf");
	rmcommand.arg(&args.tmp);

	let mut spawned = rmcommand.stdout(Stdio::piped()).spawn()?;

	let exit_status = spawned
		.wait()
		.expect("Something went wrong while waiting for \"rm\" to finish... (Did it even run?)");

	if !exit_status.success() {
		return Err(ioError::new(
			ErrorKind::Other,
			"\"rm\" exited with a non-zero status, Stopping YT-DL-Rust",
		));
	}

	return Ok(());
}
