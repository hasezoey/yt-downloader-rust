pub enum Error {
	ioError(std::io::Error),
	fromStringUTF8Error(std::string::FromUtf8Error),
	commandNotSuccesfull(String),
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		return Self::ioError(err);
	}
}

impl From<std::string::FromUtf8Error> for Error {
	fn from(err: std::string::FromUtf8Error) -> Self {
		return Self::fromStringUTF8Error(err);
	}
}
