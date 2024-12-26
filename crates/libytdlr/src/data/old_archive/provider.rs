//! Module for the [`Provider`] Struct
//! This file contains the Struct for Provider's v1

use serde::Deserialize;

use crate::data::UNKNOWN;

/// All Providers from ytdl which need custom handling
#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct Provider(String);

impl Provider {
	/// Get current current [`Provider`] as a str
	#[must_use]
	pub fn as_str(&self) -> &str {
		return &self.0;
	}
}

impl From<&Provider> for String {
	fn from(provider: &Provider) -> Self {
		return provider.0.clone();
	}
}

impl<T: AsRef<str>> From<T> for Provider {
	fn from(v: T) -> Self {
		let mut lower = v.as_ref().to_lowercase();

		if lower.is_empty() {
			lower.push_str(UNKNOWN);
		}

		return Self(lower);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_into_string() {
		assert_eq!(String::from(UNKNOWN), String::from(&Provider(UNKNOWN.into())));
		assert_eq!(String::from("youtube"), String::from(&Provider("youtube".into())));
		assert_eq!(
			String::from("other different"),
			String::from(&Provider("other different".to_owned()))
		);
	}

	#[test]
	fn test_from_strings() {
		assert_eq!(Provider(UNKNOWN.into()), Provider::from(""));
		assert_eq!(Provider(UNKNOWN.into()), Provider::from(String::new()));

		assert_eq!(Provider(UNKNOWN.into()), Provider::from(UNKNOWN));
		assert_eq!(Provider(UNKNOWN.into()), Provider::from(String::from(UNKNOWN)));

		assert_eq!(Provider("youtube".into()), Provider::from("Youtube"));
		assert_eq!(Provider("youtube".into()), Provider::from(String::from("Youtube")));

		assert_eq!(Provider("other different".into()), Provider::from("other different"));
		assert_eq!(
			Provider("other different".into()),
			Provider::from(String::from("other different"))
		);
	}

	#[test]
	fn test_clone() {
		assert_eq!(Provider("youtube".into()), Provider("youtube".into()));
	}
}
