//! Module for utility functions, that may be used in various other modules

use std::path::{
	Path,
	PathBuf,
};

use path_absolutize::Absolutize;

/// Simple helper to resolve "~" to the Home directory
/// System agnostic as long as [`dirs::home_dir`] support's it
pub fn expand_tidle<I: AsRef<Path>>(input: I) -> Option<PathBuf> {
	let path = input.as_ref();

	if !path.starts_with("~") {
		return Some(path.to_owned());
	}
	if path == Path::new("~") {
		return dirs::home_dir();
	}
	// dont support "~user" syntax
	if !path.starts_with("~/") {
		unreachable!("This should never occur, because \"path.starts_with\" should have already returned");
	}

	return dirs::home_dir().map(|mut v| {
		// handle case where "home_dir" might be set to the root POSIX directory
		return if v == Path::new("/") {
			// "unwrap" can be used, because it is already checked that the variable starts with value
			path.strip_prefix("~").unwrap().to_owned() // return the input path, just without "~"
		} else {
			// "unwrap" can be used, because it is already checked that the variable starts with value
			v.push(path.strip_prefix("~/").unwrap());
			v
		};
	});
}

/// Convert input path to a absolute path, without hitting the filesystem.
/// This function handles `~`(home)
///
/// If the start is not absolute, CWD will be used.
///
/// This functions behavior:
/// - `/path/to/inner/../somewhere` -> `/path/to/somewhere`
/// - `relative/to/somewhere` -> `CWD/relative/to/somewhere`
/// - `./somewhere/./path` -> `CWD/somewhere/path`
/// - `~/somewhere/in/home` -> `HOME/somewhere/in/home`
pub fn to_absolute<P: AsRef<Path>>(input: P) -> std::io::Result<PathBuf> {
	let Some(converted) = expand_tidle(input) else {
		return Err(std::io::Error::new(
			std::io::ErrorKind::InvalidInput,
			"Could not resolve \"~\"",
		));
	};

	return converted.absolutize().map(|v| return v.to_path_buf());
}

#[cfg(test)]
mod test {
	use super::*;

	mod expand_tidle {
		use super::*;

		#[test]
		fn basic_func() {
			// fake home
			unsafe { std::env::set_var("HOME", "/custom/home") };

			// should not modify a absolute path
			let absolue_path = PathBuf::from("/absolute/to/path");
			assert_eq!(
				absolue_path,
				expand_tidle(&absolue_path).expect("Expected to return a SOME value")
			);

			// should not modify a relative path
			let relative_path = PathBuf::from("./inner/path");
			assert_eq!(
				relative_path,
				expand_tidle(&relative_path).expect("Expected to return a SOME value")
			);

			// should resolve "~" without extra paths
			let home_no_extensions = PathBuf::from("~");
			assert_eq!(
				dirs::home_dir().expect("Expected to return a SOME value"),
				expand_tidle(home_no_extensions).expect("Expected to return a SOME value")
			);

			// should resolve "~" with extra paths
			let home_with_extensions = PathBuf::from("~/some/path");
			assert_eq!(
				Path::join(&dirs::home_dir().expect("Expected to return a SOME value"), "some/path"),
				expand_tidle(home_with_extensions).expect("Expected to return a SOME value")
			);

			// should return weird path "~user"
			let weird_path = PathBuf::from("~user");
			assert_eq!(
				weird_path,
				expand_tidle(&weird_path).expect("Expected to return a SOME value")
			);
		}
	}

	mod to_absolute {
		use super::*;

		#[test]
		fn basic_func() {
			// fake home
			unsafe { std::env::set_var("HOME", "/custom/home") };

			// should not modify the input
			let absolue_path = PathBuf::from("/absolute/to/path");
			assert_eq!(
				absolue_path,
				to_absolute(&absolue_path).expect("Expected to return a OK value")
			);

			// should modify the input, but not the base
			let absolue_containing_relative = PathBuf::from("/absolute/to/inner/../path");
			assert_eq!(
				absolue_path,
				to_absolute(absolue_containing_relative).expect("Expected to return a OK value")
			);

			// should add CWD as a base
			let relative_path = PathBuf::from("./inner/path");
			assert_eq!(
				Path::join(&std::env::current_dir().expect("Expected to have a CWD"), "inner/path"),
				to_absolute(relative_path).expect("Expected to return a OK value")
			);

			// should resolve a "~"
			let relative_home = PathBuf::from("~/inner/path");
			assert_eq!(
				Path::join(&dirs::home_dir().expect("Expected to have HOME"), "inner/path"),
				to_absolute(relative_home).expect("Expected to return a OK value")
			);
		}
	}
}
