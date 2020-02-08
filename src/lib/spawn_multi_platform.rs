use std::process::Command;

/// Spawn "youtube-dl" from PATH
#[cfg(target_os = "linux")]
pub fn spawn_command() -> Command {
	return Command::new("youtube-dl");
}

/// Spawn "youtube-dl" for non-linux systems
#[cfg(not(target_os = "linux"))]
pub fn spawn_command() -> Command {
	use std::env::current_exe;
	use std::fs::metadata;

	let path: Path = current_exe()
		.expect("Current Exectuable path not found, how does this even run?")
		.parent()
		.expect("invalid executable folder");

	if cfg!(target_os = windows) {
		path.join("youtube-dl.exe");
	} else {
		path.join("youtube-dl");
	}

	if let Ok(_) = metadata(path) {
		return Command::new(path);
	}

	return Command::new("youtube-dl");
}
