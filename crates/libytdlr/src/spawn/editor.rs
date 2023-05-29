//! Module that contains all logic for spawning the "editor" command
use std::{
	path::Path,
	process::Command,
};

#[inline]
pub fn base_editor(editor: &Path, filepath: &Path) -> Command {
	let mut cmd = super::multiplatform::spawn_command(&editor);
	cmd.arg(filepath);

	return cmd;
}
