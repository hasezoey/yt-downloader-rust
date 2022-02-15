//! Module for the Error type this library uses
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
	/// Variant for when a Unexpected EOF happened (like in import)
	UnexpectedEOF(String),
	/// Variant for serde-json Errors
	SerdeJSONError(serde_json::Error),
	/// Variant for Other messages
	Other(String),
}

// this is custom, because "std::io::Error" does not implement "PartialEq", but "std::io::ErrorKind" does
impl PartialEq for Error {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::IoError(l0), Self::IoError(r0)) => return l0.kind() == r0.kind(),
			(Self::FromStringUTF8Error(l0), Self::FromStringUTF8Error(r0)) => return l0 == r0,
			(Self::CommandNotSuccesfull(l0), Self::CommandNotSuccesfull(r0)) => return l0 == r0,
			(Self::NoCapturesFound(l0), Self::NoCapturesFound(r0)) => return l0 == r0,
			(Self::Other(l0), Self::Other(r0)) => return l0 == r0,
			(Self::UnexpectedEOF(l0), Self::UnexpectedEOF(r0)) => return l0 == r0,
			// Always return "false" for a serde_json::Error
			(Self::SerdeJSONError(_l0), Self::SerdeJSONError(_r0)) => return false,
			(_, _) => return false,
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

impl From<serde_json::Error> for Error {
	fn from(err: serde_json::Error) -> Self {
		return Self::SerdeJSONError(err);
	}
}

impl From<Error> for std::io::Error {
	fn from(err: Error) -> Self {
		return std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err));
	}
}

impl Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		return write!(
			f,
			"{}",
			match &self {
				Self::CommandNotSuccesfull(s) => format!("CommandNotSuccessfull: {}", s),
				Self::NoCapturesFound(s) => format!("NoCapturesFound: {}", s),
				Self::FromStringUTF8Error(v) => format!("FromStringUTF8Error: {}", v),
				Self::IoError(v) => format!("IoError: {}", v),
				Self::UnexpectedEOF(v) => format!("UnexpectedEOF: {}", v),
				Self::SerdeJSONError(v) => format!("SerdeJSONError: {}", v),
				Self::Other(v) => format!("Other: {}", v),
			}
		);
	}
}
