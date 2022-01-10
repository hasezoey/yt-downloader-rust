use std::process::Command;
use std::process::{
	Output,
	Stdio,
};

#[inline]
pub fn base_ytdl() -> Command {
	return super::multiplatform::spawn_command("youtube-dl");
}

/// Get Version of `ffmpeg`
#[inline]
pub fn ytdl_version() -> Result<String, crate::Error> {
	let mut cmd = base_ytdl();
	cmd.arg("--version");

	let command_output: Output = cmd
		.stderr(Stdio::null())
		.stdout(Stdio::piped())
		.stdin(Stdio::null())
		.spawn()?
		.wait_with_output()?;

	if !command_output.status.success() {
		return Err(crate::Error::CommandNotSuccesfull(
			"FFMPEG did not successfully exit!".to_string(),
		));
	}

	return Ok(String::from_utf8(command_output.stdout)?);
}

pub fn ytdl_available() -> bool {
	let ytdl_version = match ytdl_version() {
		Err(_) => return false,
		Ok(v) => v,
	};
	todo!()
}
