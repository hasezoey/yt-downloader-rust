use std::fmt::Display;

/// Error type for "yt-downloader-rust", implements all Error types that could happen in this lib
#[derive(Debug)]
pub enum Error {
	/// Wrapper Variant for [`std::io::Error`]
	IoError(std::io::Error),
	/// Wrapper Variant for [`std::string::FromUtf8Error`]
	FromStringUTF8Error(std::string::FromUtf8Error),
	/// Variant for when a spawned command was not successfull
	CommandNotSuccesfull(String),
	/// Variant for when no regex captures have been found
	NoCapturesFound(String),
}

// this is custom, because "std::io::Error" does not implement "PartialEq", but "std::io::ErrorKind" does
impl PartialEq for Error {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::IoError(l0), Self::IoError(r0)) => l0.kind() == r0.kind(),
			(Self::FromStringUTF8Error(l0), Self::FromStringUTF8Error(r0)) => l0 == r0,
			(Self::CommandNotSuccesfull(l0), Self::CommandNotSuccesfull(r0)) => l0 == r0,
			(Self::NoCapturesFound(l0), Self::NoCapturesFound(r0)) => l0 == r0,
			(_, _) => false,
		}
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		return Self::IoError(err);
	}
}

impl From<std::string::FromUtf8Error> for Error {
	fn from(err: std::string::FromUtf8Error) -> Self {
		return Self::FromStringUTF8Error(err);
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{}",
			match &self {
				Self::CommandNotSuccesfull(s) => format!("CommandNotSuccessfull: {}", s),
				Self::NoCapturesFound(s) => format!("NoCapturesFound: {}", s),
				Self::FromStringUTF8Error(v) => format!("FromStringUTF8Error: {}", v),
				Self::IoError(v) => format!("IoError: {}", v),
			}
		)
	}
}
