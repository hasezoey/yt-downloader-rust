//! Module for the Error type this library uses

/// Error type for "yt-downloader-rust", implements all Error types that could happen in this lib
#[derive(thiserror::Error, Debug)]
pub enum Error {
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
	/// Variant for a Unexpected Process Exit (like when ytdl fails to spawn)
	#[error("UnexpectedProcessExit: {0}")]
	UnexpectedProcessExit(String),
	/// Variant for a diesel Connection Error (sql i/o)
	#[error("SQLConnectionError: {0}")]
	SQLConnectionError(#[from] diesel::ConnectionError),
	/// Variant for a diesel SQL Operation Error
	#[error("SQLOperationError: {0}")]
	SQLOperationError(#[from] diesel::result::Error),
}

impl Error {
	pub fn other<M>(msg: M) -> Self
	where
		M: Into<String>,
	{
		return Self::Other(msg.into());
	}
}

// this is custom, some errors like "std::io::Error" do not implement "PartialEq", but some inner type may do
impl PartialEq for Error {
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
			// Always return "false" for a Unexpected Process Exit
			(Self::UnexpectedProcessExit(_l0), Self::UnexpectedProcessExit(_r0)) => return false,
			(_, _) => return false,
		}
	}
}
