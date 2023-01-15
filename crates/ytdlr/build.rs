use std::process::Command;

fn main() {
	// set what version string to use for the build
	// currently it depends on what git outputs, or if failed use "unknown"
	{
		let version = Command::new("git")
			.args(&["describe", "--tags", "--always", "--dirty"])
			.output()
			.ok()
			.map(|v| String::from_utf8(v.stdout).ok())
			.flatten()
			.unwrap_or(String::from("unknown"));
		println!("cargo:rustc-env=YTDLR_VERSION={}", version);
	}
}
