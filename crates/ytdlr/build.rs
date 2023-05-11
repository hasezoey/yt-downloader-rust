use std::process::Command;

fn main() {
	// set what version string to use for the build
	// currently it depends on what git outputs, or if failed use "unknown"
	{
		println!("cargo:rerun-if-changed=build.rs");
		println!("cargo:rerun-if-changed=.git/HEAD");

		let version = Command::new("git")
			.args(["describe", "--tags", "--always", "--dirty"])
			.output()
			.ok()
			.and_then(|v| return String::from_utf8(v.stdout).ok())
			.unwrap_or(String::from("unknown"));
		println!("cargo:rustc-env=YTDLR_VERSION={version}");
	}
}
