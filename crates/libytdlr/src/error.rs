//! Module for the Error type this library uses

use std::{
	backtrace::Backtrace,
	io::Error as ioError,
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

	/// Create a custom [ioError] with this [Error] wrapped around
	pub fn custom_ioerror<M>(kind: std::io::ErrorKind, msg: M) -> Self
	where
		M: Into<String>,
	{
		return Self::new(ErrorInner::IoError(ioError::new(kind, msg.into())));
	}

	fn_string!(other, ErrorInner::Other);
	fn_string!(no_captures, ErrorInner::NoCapturesFound);
	fn_string!(unexpected_eof, ErrorInner::UnexpectedEOF);
	fn_string!(command_unsuccessful, ErrorInner::CommandNotSuccesfull);
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
	#[error("IoError: {0}")]
	IoError(#[from] std::io::Error),
	/// Wrapper Variant for [`std::string::FromUtf8Error`]
	#[error("FromStringUTF8Error: {0}")]
	FromStringUTF8Error(#[from] std::string::FromUtf8Error),
	/// Variant for when a spawned command was not successfull
	#[error("CommandNotSuccessfull: {0}")]
	CommandNotSuccesfull(String),
	/// Variant for when no regex captures have been found
	#[error("NoCapturesFound: {0}")]
	NoCapturesFound(String),
	/// Variant for when a Unexpected EOF happened (like in import)
	#[error("UnexpectedEOF: {0}")]
	UnexpectedEOF(String),
	/// Variant for serde-json Errors
	#[error("SerdeJSONError: {0}")]
	SerdeJSONError(#[from] serde_json::Error),
	/// Variant for Other messages
	#[error("Other: {0}")]
	Other(String),
	/// Variant for a diesel Connection Error (sql i/o)
	#[error("SQLConnectionError: {0}")]
	SQLConnectionError(#[from] diesel::ConnectionError),
	/// Variant for a diesel SQL Operation Error
	#[error("SQLOperationError: {0}")]
	SQLOperationError(#[from] diesel::result::Error),
}

// this is custom, some errors like "std::io::Error" do not implement "PartialEq", but some inner type may do
impl PartialEq for ErrorInner {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::IoError(l0), Self::IoError(r0)) => return l0.kind() == r0.kind(),
			(Self::FromStringUTF8Error(l0), Self::FromStringUTF8Error(r0)) => return l0 == r0,
			(Self::CommandNotSuccesfull(l0), Self::CommandNotSuccesfull(r0)) => return l0 == r0,
			(Self::NoCapturesFound(l0), Self::NoCapturesFound(r0)) => return l0 == r0,
			(Self::Other(l0), Self::Other(r0)) => return l0 == r0,
			(Self::UnexpectedEOF(l0), Self::UnexpectedEOF(r0)) => return l0 == r0,
			(Self::SQLConnectionError(l0), Self::SQLConnectionError(r0)) => return l0 == r0,
			(Self::SQLOperationError(l0), Self::SQLOperationError(r0)) => return l0 == r0,
			// Always return "false" for a serde_json::Error
			(Self::SerdeJSONError(_l0), Self::SerdeJSONError(_r0)) => return false,
			(_, _) => return false,
		}
	}
}
