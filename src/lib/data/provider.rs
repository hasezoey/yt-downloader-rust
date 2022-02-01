//! Module for the [`Provider`] Struct
//! This file contains the Struct for Provider's v1

use serde::{
	Deserialize,
	Serialize,
};
use std::fmt;

/// All Providers from ytdl which need custom handling
#[derive(Debug, PartialEq, Clone)]
#[non_exhaustive]
pub enum Provider {
	Youtube,
	Unknown,
	Other(String),
}

impl From<&Provider> for String {
	fn from(provider: &Provider) -> Self {
		return match provider {
			Provider::Youtube => "youtube".to_owned(),
			Provider::Unknown => "unknown".to_owned(),
			Provider::Other(d) => d.to_lowercase(),
		};
	}
}

impl<T: AsRef<str>> From<T> for Provider {
	fn from(v: T) -> Self {
		let lower = v.as_ref().to_lowercase();

		return match lower.as_str() {
			"youtube" => Provider::Youtube,
			"" | "unknown" => Provider::Unknown,
			_ => Provider::Other(lower),
		};
	}
}

impl fmt::Display for Provider {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		return write!(f, "{}", String::from(self));
	}
}

impl Default for Provider {
	fn default() -> Provider {
		return Provider::Unknown;
	}
}

impl Serialize for Provider {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		return serializer.serialize_str(&String::from(self));
	}
}

impl<'de> Deserialize<'de> for Provider {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct ProviderVisitor;

		impl<'de> serde::de::Visitor<'de> for ProviderVisitor {
			type Value = Provider;

			// {"provider": "something"} will always result in an str
			fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
				return Ok(Provider::from(v));
			}

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				write!(formatter, "an String to be parsed into an Provider-Variant")?;

				return Ok(());
			}
		}

		return deserializer.deserialize_str(ProviderVisitor);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_into_string() {
		assert_eq!(String::from("unknown"), String::from(&Provider::Unknown));
		assert_eq!(String::from("youtube"), String::from(&Provider::Youtube));
		assert_eq!(
			String::from("other different"),
			String::from(&Provider::Other("other different".to_owned()))
		);
	}

	#[test]
	fn test_display() {
		assert_eq!(String::from("unknown"), format!("{}", &Provider::Unknown));
		assert_eq!(String::from("youtube"), format!("{}", &Provider::Youtube));
		assert_eq!(
			String::from("other different"),
			format!("{}", &Provider::Other("other different".to_owned()))
		);
	}

	#[test]
	fn test_from_strings() {
		assert_eq!(Provider::Unknown, Provider::from(""));
		assert_eq!(Provider::Unknown, Provider::from(String::from("")));

		assert_eq!(Provider::Unknown, Provider::from("Unknown"));
		assert_eq!(Provider::Unknown, Provider::from(String::from("Unknown")));

		assert_eq!(Provider::Youtube, Provider::from("Youtube"));
		assert_eq!(Provider::Youtube, Provider::from(String::from("Youtube")));

		assert_eq!(
			Provider::Other("other different".to_owned()),
			Provider::from("other different")
		);
		assert_eq!(
			Provider::Other("other different".to_owned()),
			Provider::from(String::from("other different"))
		);
	}

	#[test]
	fn test_default() {
		assert_eq!(Provider::Unknown, Provider::default());
	}

	#[test]
	fn test_clone() {
		assert_eq!(Provider::Youtube, Provider::Youtube.clone());
	}

	mod serde {
		use super::*;
		use serde_test::*;

		#[test]
		fn test_serialize_both() {
			assert_tokens(&Provider::Unknown, &[Token::String("unknown")]);
			assert_tokens(&Provider::Youtube, &[Token::String("youtube")]);
			assert_tokens(&Provider::Other("spotify".to_owned()), &[Token::String("spotify")]);
		}
	}
}
