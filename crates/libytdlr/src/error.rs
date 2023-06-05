//! Module for the Error type this library uses

use std::{
	backtrace::Backtrace,
	io::Error as ioError,
	path::Path,
	thread::JoinHandle,
};

/// Macro to not repeat having to do multiple implementations of a [ErrorInner] variant with the same string type
macro_rules! fn_string {
	($fn_name:ident, $fortype:expr) => {
		#[doc = concat!("Create a new [Self] as [", stringify!($fortype), "]")]
		pub fn $fn_name<M>(msg: M) -> Self
		where
			M: Into<String>,
		{
			return Self::new($fortype(msg.into()));
		}
	};
}

/// Macro to not repeat having to do multiple implementations of a [ErrorInner] variant with the same path type
macro_rules! fn_path {
	($fn_name:ident, $fortype:expr) => {
		#[doc = concat!("Create a new [Self] as [", stringify!($fortype), "]")]
		pub fn $fn_name<M, P>(msg: M, path: P) -> Self
		where
			M: Into<String>,
			P: AsRef<Path>,
		{
			return Self::new($fortype(msg.into(), path.as_ref().to_string_lossy().to_string()));
		}
	};
}

// TODO: change backtrace implementation to be by thiserror, if possible once features become stable
// error_generic_member_access https://github.com/rust-lang/rust/issues/99301
// provide_any https://github.com/rust-lang/rust/issues/96024

/// Error type for libytdlr, contains a backtrace, wrapper around [ErrorInner]
#[derive(Debug)]
pub struct Error {
	/// The actual error
	source:    ErrorInner,
	/// The backtrace for the error
	backtrace: Backtrace,
}

impl Error {
	/// Construct a new [Error] instance based on [ErrorInner]
	pub fn new(source: ErrorInner) -> Self {
		return Self {
			source,
			backtrace: Backtrace::capture(),
		};
	}

	/// Get the backtrace that is stored
	pub fn get_backtrace(&self) -> &Backtrace {
		return &self.backtrace;
	}

	/// Create a custom [ioError] with this [Error] wrapped around with a [Path] attached
	pub fn custom_ioerror_path<M, P>(kind: std::io::ErrorKind, msg: M, path: P) -> Self
	where
		M: Into<String>,
		P: AsRef<Path>,
	{
		return Self::new(ErrorInner::IoError(
			ioError::new(kind, msg.into()),
			format_path(path.as_ref().to_string_lossy().to_string()),
		));
	}

	/// Create a custom [ioError] with this [Error] wrapped around with a location attached
	pub fn custom_ioerror_location<M, L>(kind: std::io::ErrorKind, msg: M, location: L) -> Self
	where
		M: Into<String>,
		L: AsRef<str>,
	{
		return Self::new(ErrorInner::IoError(
			ioError::new(kind, msg.into()),
			format_location(location.as_ref()),
		));
	}

	fn_string!(other, ErrorInner::Other);
	fn_string!(no_captures, ErrorInner::NoCapturesFound);
	fn_string!(unexpected_eof, ErrorInner::UnexpectedEOF);
	fn_string!(command_unsuccessful, ErrorInner::CommandNotSuccesful);
	fn_path!(not_a_directory, ErrorInner::NotADirectory);
	fn_path!(not_a_file, ErrorInner::NotAFile);

	/// Map a [std::thread::JoinHandle::join] error to a [Error] with a thread name
	fn map_thread_join<N: AsRef<str>>(name: N) -> impl Fn(Box<dyn std::any::Any + Send + 'static>) -> Self {
		return move |from| {
			let name = name.as_ref().to_owned();
			if let Some(v) = from.downcast_ref::<String>() {
				return Self::new(ErrorInner::ThreadJoinError(v.clone(), name));
			}
			if let Some(v) = from.downcast_ref::<&str>() {
				return Self::new(ErrorInner::ThreadJoinError(v.to_string(), name));
			}

			return Self::new(ErrorInner::ThreadJoinError("unknown error".into(), name));
		};
	}
}

impl PartialEq for Error {
	fn eq(&self, other: &Self) -> bool {
		return self.source == other.source;
	}
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		return self.source.fmt(f);
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		return self.source.source();
	}
}

// implement all From<> variants that ErrorInner also implements
impl<T> From<T> for Error
where
	T: Into<ErrorInner>,
{
	fn from(value: T) -> Self {
		return Self::new(value.into());
	}
}

/// Error type for "yt-downloader-rust", implements all Error types that could happen in this lib
#[derive(thiserror::Error, Debug)]
pub enum ErrorInner {
	/// Wrapper Variant for [`std::io::Error`]
	/// Argument 1 (String) is up to the implementation to set, commonly the path
	#[error("IoError: {0}; {1}")]
	IoError(std::io::Error, String),
	/// Wrapper Variant for [`std::string::FromUtf8Error`]
	#[error("FromStringUTF8Error: {0}")]
	FromStringUTF8Error(#[from] std::string::FromUtf8Error),
	/// Variant for serde-json Errors
	#[error("SerdeJSONError: {0}")]
	SerdeJSONError(#[from] serde_json::Error),

	/// Variant for a diesel Connection Error (sql i/o)
	#[error("SQLConnectionError: {0}")]
	SQLConnectionError(#[from] diesel::ConnectionError),
	/// Variant for a diesel SQL Operation Error
	#[error("SQLOperationError: {0}")]
	SQLOperationError(#[from] diesel::result::Error),

	/// Variant for when a spawned command was not successfull
	#[error("CommandNotSuccessfull: {0}")]
	CommandNotSuccesful(String),
	/// Variant for when no regex captures have been found
	#[error("NoCapturesFound: {0}")]
	NoCapturesFound(String),
	/// Variant for when a Unexpected EOF happened (like in import)
	#[error("UnexpectedEOF: {0}")]
	UnexpectedEOF(String),
	/// Variant for when a directory path was expected but did not exist yet or was not a directory
	/// TODO: replace with io::ErrorKind::NotADirectory once stable <https://github.com/rust-lang/rust/issues/86442>
	#[error("NotADirectory: {0}; Path: \"{1}\"")]
	NotADirectory(String, String),
	/// Variant for when a file path was expected but did not exist yet or was not a file
	#[error("NotAFile: {0}; Path: \"{1}\"")]
	NotAFile(String, String),
	/// Variant for thread join errors
	#[error("ThreadJoinError: name: \"{1}\" original error: {0}")]
	ThreadJoinError(String, String),
	/// Variant for Other messages
	#[error("Other: {0}")]
	Other(String),
}

// this is custom, some errors like "std::io::Error" do not implement "PartialEq", but some inner type may do
impl PartialEq for ErrorInner {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::IoError(l0, l1), Self::IoError(r0, r1)) => return l0.kind() == r0.kind() && l1 == r1,
			(Self::FromStringUTF8Error(l0), Self::FromStringUTF8Error(r0)) => return l0 == r0,
			(Self::SQLConnectionError(l0), Self::SQLConnectionError(r0)) => return l0 == r0,
			(Self::SQLOperationError(l0), Self::SQLOperationError(r0)) => return l0 == r0,

			(Self::CommandNotSuccesful(l0), Self::CommandNotSuccesful(r0)) => return l0 == r0,
			(Self::NoCapturesFound(l0), Self::NoCapturesFound(r0)) => return l0 == r0,
			(Self::Other(l0), Self::Other(r0)) => return l0 == r0,
			(Self::UnexpectedEOF(l0), Self::UnexpectedEOF(r0)) => return l0 == r0,
			(Self::NotADirectory(l0, l1), Self::NotADirectory(r0, r1)) => return l0 == r0 && l1 == r1,
			(Self::NotAFile(l0, l1), Self::NotAFile(r0, r1)) => return l0 == r0 && l1 == r1,

			(_, _) => return false,
		}
	}
}

/// Custom [std::thread::JoinHandle::join] implementation to return a [Error] with thread name
pub trait CustomThreadJoin<T> {
	/// Custom thread join method for libytdlr so that errors are automatically mapped to the current error type and have the named from the thread
	fn join_err(self) -> Result<T, crate::Error>;
}

impl<T> CustomThreadJoin<T> for JoinHandle<T> {
	fn join_err(self) -> Result<T, crate::Error> {
		let name = self.thread().name().unwrap_or("<unnamed>").to_owned();
		return self.join().map_err(crate::Error::map_thread_join(name));
	}
}

/// Helper function to keep consistent formatting
#[inline]
fn format_path(msg: String) -> String {
	return format!("Path \"{}\"", msg);
}
/// Helper function to keep consistent formatting
#[inline]
fn format_location(msg: &str) -> String {
	return format!("Location \"{}\"", msg);
}

/// Trait to map [std::io::Error] into [Error]
pub trait IOErrorToError<T> {
	/// Map a [std::io::Error] to [Error] with a [std::path::Path] attached
	fn attach_path_err<P: AsRef<Path>>(self, path: P) -> Result<T, crate::Error>;
	/// Map a [std::io::Error] to [Error] with a location attached (for when [attach_path_err] is not applicable)
	fn attach_location_err<P: AsRef<str>>(self, pipe_msg: P) -> Result<T, crate::Error>;
}

impl<T> IOErrorToError<T> for Result<T, std::io::Error> {
	fn attach_path_err<P: AsRef<Path>>(self, path: P) -> Result<T, crate::Error> {
		return match self {
			Ok(v) => Ok(v),
			Err(e) => Err(crate::Error::new(ErrorInner::IoError(
				e,
				format_path(path.as_ref().to_string_lossy().to_string()),
			))),
		};
	}

	fn attach_location_err<L: AsRef<str>>(self, location: L) -> Result<T, crate::Error> {
		return match self {
			Ok(v) => Ok(v),
			Err(e) => Err(crate::Error::new(ErrorInner::IoError(
				e,
				format_location(location.as_ref()),
			))),
		};
	}
}
