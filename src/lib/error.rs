pub enum Error {
	IoError(std::io::Error),
	FromStringUTF8Error(std::string::FromUtf8Error),
	CommandNotSuccesfull(String),
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
