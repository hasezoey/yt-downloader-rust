use shellexpand::tilde;
use std::io::{
	Error as ioError,
	ErrorKind,
	Result as ioResult,
};
use std::path::{
	Component,
	Path,
	PathBuf,
};

/// This Function is copied from Cargo [paths.rs](https://github.com/rust-lang/cargo/blob/070e459c2d8b79c5b2ac5218064e7603329c92ae/crates/cargo-util/src/paths.rs)  
/// The project might be licended under MIT  
/// TODO: replace with official implementation when available  
///
/// Normalize a path, removing things like `.` and `..`.
///
/// CAUTION: This does not resolve symlinks (unlike
/// [`std::fs::canonicalize`]). This may cause incorrect or surprising
/// behavior at times. This should be used carefully. Unfortunately,
/// [`std::fs::canonicalize`] can be hard to use correctly, since it can often
/// fail, or on Windows returns annoying device paths. This is a problem Cargo
/// needs to improve on.
pub fn normalize_path(path: &Path) -> PathBuf {
	let mut components = path.components().peekable();
	let mut ret = if let Some(c @ Component::Prefix(..)) = components.peek().cloned() {
		components.next();
		PathBuf::from(c.as_os_str())
	} else {
		PathBuf::new()
	};

	for component in components {
		match component {
			Component::Prefix(..) => unreachable!(),
			Component::RootDir => {
				ret.push(component.as_os_str());
			},
			Component::CurDir => {},
			Component::ParentDir => {
				ret.pop();
			},
			Component::Normal(c) => {
				ret.push(c);
			},
		}
	}
	return ret;
}

/// Convert `target` into an absolute path with `base` as a base.  
/// If `target` is already absolute, it returns `target` as is.
/// # Errors
/// This Function errors if `base` is not absolute with an [`ioError`]  
/// This Function also errors if the input paths are not an valid [`str`]
pub fn to_absolute(base: &Path, target: &Path) -> ioResult<PathBuf> {
	let base_fmt: PathBuf = tilde(
		base.to_str()
			.ok_or_else(|| return ioError::new(ErrorKind::InvalidData, "Base Path is not an valid str"))?,
	)
	.as_ref()
	.into();
	let target_fmt: PathBuf = tilde(
		target
			.to_str()
			.ok_or_else(|| return ioError::new(ErrorKind::InvalidData, "Target Path is not an valid str"))?,
	)
	.as_ref()
	.into();

	if target_fmt.is_absolute() {
		return Ok(target_fmt);
	}

	if !base_fmt.is_absolute() {
		return Err(ioError::new(ErrorKind::InvalidInput, "Base Path is not absolute!"));
	}

	return Ok(normalize_path(base_fmt.join(&target_fmt).as_ref()));
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_normalize_path() {
		let input_path = Path::new("hello/./../hello2/something");
		assert_eq!(PathBuf::from("hello2/something"), normalize_path(input_path));
	}

	#[test]
	fn test_to_absolute() -> ioResult<()> {
		let base_path = Path::new("/root/to/something");
		let test_target_empty = Path::new("");
		let test_target_1 = Path::new("/absolute/target");
		let test_target_2 = Path::new("../../path/to/somewhere/./else");
		// TODO: add test for "to_absolute" and tilde ("~/home/dir") when using dirs-next

		// should return "base_path" unmodified
		assert_eq!(PathBuf::from(&base_path), to_absolute(base_path, test_target_empty)?);
		// should return "target" without "base_path" because its absolute
		assert_eq!(PathBuf::from(&test_target_1), to_absolute(base_path, test_target_1)?);
		// should return combined "base_path" and "target"
		assert_eq!(
			PathBuf::from("/root/path/to/somewhere/else"),
			to_absolute(base_path, test_target_2)?
		);

		// should return an Error because base is not absolute
		{
			let returned = to_absolute(test_target_2, test_target_empty).unwrap_err();
			assert_eq!(ErrorKind::InvalidInput, returned.kind());
			assert_eq!("Base Path is not absolute!", returned.to_string());
		}

		return Ok(());
	}
}
