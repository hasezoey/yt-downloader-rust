//! Module that contains all logic for spawning the "editor" command
use std::{
	path::Path,
	process::Command,
};

/// Create a new editor instance with the given filepath as a argument
#[inline]
#[must_use]
pub fn base_editor(editor: &Path, filepath: &Path) -> Command {
	let mut cmd = Command::new(editor);
	cmd.arg(filepath);

	return cmd;
}
