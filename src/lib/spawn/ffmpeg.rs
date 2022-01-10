use std::process::Command;
use std::process::{
	Output,
	Stdio,
};

#[inline]
pub fn base_ffmpeg(overwrite: bool) -> Command {
	let mut cmd = super::multiplatform::spawn_command("ffmpeg");

	if overwrite {
		cmd.arg("-y"); // always overwrite output path
	}

	return cmd;
}

/// Get Version of `ffmpeg`
#[inline]
pub fn ffmpeg_version() -> Result<String, crate::Error> {
	let mut cmd = base_ffmpeg(false);
	cmd.arg("-version");

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

pub fn ffmpeg_available() -> bool {
	let ffmpeg_version = match ffmpeg_version() {
		Err(_) => return false,
		Ok(v) => v,
	};
	todo!()
}
