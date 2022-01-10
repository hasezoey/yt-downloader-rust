use std::process::Command;

/// Spawn "youtube-dl" from PATH
#[cfg(target_os = "linux")]
#[inline]
pub fn spawn_command(binary_name: &str) -> Command {
	return Command::new(binary_name);
}

/// Spawn "youtube-dl" for non-linux systems
#[cfg(not(target_os = "linux"))]
pub fn spawn_command(binary_name: &str) -> Command {
	use std::env::current_exe;
	use std::path::{
		Path,
		PathBuf,
	};

	let current_binary_path: PathBuf =
		current_exe().expect("Current Exectuable path not found, how does this even run?");
	let current_binary_directory: &Path = current_binary_path.parent().expect("invalid executable folder");

	let binary = if cfg!(windows) {
		current_binary_directory.join(format!("{}.exe", binary_name))
	} else {
		current_binary_directory.join(binary_name)
	};

	return Command::new(binary);
}
