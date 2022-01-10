use std::path::Path;
use std::process::Command;

#[inline]
pub fn base_editor(editor: &str, filepath: &Path) -> Command {
	let mut cmd = super::multiplatform::spawn_command(editor);
	cmd.arg(filepath);

	return cmd;
}

pub fn editor_available() -> bool {
	todo!()
}
