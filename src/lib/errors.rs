use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct GenericError {
	details: String,
}

impl GenericError {
	pub fn new<S: Into<String>>(msg: S) -> GenericError {
		return GenericError { details: msg.into() };
	}
}

impl fmt::Display for GenericError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		return write!(f, "{}", self.details);
	}
}

impl Error for GenericError {
	fn description(&self) -> &str {
		return &self.details;
	}
}
