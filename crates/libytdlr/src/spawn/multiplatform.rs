use std::ffi::OsStr;
use std::process::Command;

// This file still exists and is seperated for future quick changes

/// Spawn a binary cross-system (not-windows version)
#[cfg(not(target_os = "windows"))]
#[inline]
pub fn spawn_command<P: AsRef<OsStr>>(binary_name: &P) -> Command {
	return Command::new(binary_name);
}

/// Spawn a binary cross-system (windows version)
/// Apparently, rust automatically adds a extensions (".exe") if none is specified
/// Also, rust automatically searches all the paths, including the ytdl-rust binary path
#[cfg(target_os = "windows")]
#[inline]
pub fn spawn_command<P: AsRef<OsStr>>(binary_name: &P) -> Command {
	return Command::new(binary_name);
}
