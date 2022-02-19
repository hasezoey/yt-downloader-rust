use std::process::Command;

// This file still exists and is seperated for future quick changes

/// Spawn "youtube-dl" in non-windows / DOS systems
#[cfg(not(target_os = "windows"))]
#[inline]
pub fn spawn_command(binary_name: &str) -> Command {
	return Command::new(binary_name);
}

/// Spawn "youtube-dl" for windows / DOS systems
/// Apparently, rust automatically adds a extensions (".exe") if none is specified
/// Also, rust automatically searches all the paths, including the ytdl-rust binary path
#[cfg(target_os = "windows")]
#[inline]
pub fn spawn_command(binary_name: &str) -> Command {
	return Command::new(binary);
}
