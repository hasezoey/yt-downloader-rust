use std::path::{
	Path,
	PathBuf,
};

// Utils file, may contain various small helper functions

/// Simple helper to resolve "~"
pub fn expand_tidle<I: AsRef<Path>>(input: I) -> Option<PathBuf> {
	let path = input.as_ref();

	if !path.starts_with("~") {
		return Some(path.to_owned());
	}
	if path == Path::new("~") {
		return dirs_next::home_dir();
	}
	// dont support "~user" syntax
	if !path.starts_with("~/") {
		warn!("Tilde(~) can only be used without usernames");
		return None;
	}

	return dirs_next::home_dir().map(|mut v| {
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
