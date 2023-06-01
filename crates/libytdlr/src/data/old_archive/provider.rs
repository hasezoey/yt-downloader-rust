//! Module for the [`Provider`] Struct
//! This file contains the Struct for Provider's v1

use serde::Deserialize;

/// All Providers from ytdl which need custom handling
#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct Provider(String);

impl From<&Provider> for String {
	fn from(provider: &Provider) -> Self {
		return provider.0.clone();
	}
}

impl<T: AsRef<str>> From<T> for Provider {
	fn from(v: T) -> Self {
		let mut lower = v.as_ref().to_lowercase();

		if lower.is_empty() {
			lower.push_str("unknown");
		}

		return Self(lower);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_into_string() {
		assert_eq!(String::from("unknown"), String::from(&Provider("unknown".into())));
		assert_eq!(String::from("youtube"), String::from(&Provider("youtube".into())));
		assert_eq!(
			String::from("other different"),
			String::from(&Provider("other different".to_owned()))
		);
	}

	#[test]
	fn test_from_strings() {
		assert_eq!(Provider("unknown".into()), Provider::from(""));
		assert_eq!(Provider("unknown".into()), Provider::from(String::from("")));

		assert_eq!(Provider("unknown".into()), Provider::from("Unknown"));
		assert_eq!(Provider("unknown".into()), Provider::from(String::from("Unknown")));

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
