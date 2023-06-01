//! Module for the [`Provider`] Struct
//! This file contains the Struct for Provider's v1

use serde::Deserialize;

/// All Providers from ytdl which need custom handling
#[derive(Debug, PartialEq, Clone)]
#[non_exhaustive]
pub enum Provider {
	Other(String),
}

impl From<&Provider> for String {
	fn from(provider: &Provider) -> Self {
		return match provider {
			Provider::Other(d) => d.to_lowercase(),
		};
	}
}

impl<T: AsRef<str>> From<T> for Provider {
	fn from(v: T) -> Self {
		let lower = v.as_ref().to_lowercase();

		return match lower.as_str() {
			"youtube" => Provider::Other("youtube".into()),
			"" | "unknown" => Provider::Other("unknown".into()),
			_ => Provider::Other(lower),
		};
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
		assert_eq!(
			String::from("unknown"),
			String::from(&Provider::Other("unknown".into()))
		);
		assert_eq!(
			String::from("youtube"),
			String::from(&Provider::Other("youtube".into()))
		);
		assert_eq!(
			String::from("other different"),
			String::from(&Provider::Other("other different".to_owned()))
		);
	}

	#[test]
	fn test_from_strings() {
		assert_eq!(Provider::Other("unknown".into()), Provider::from(""));
		assert_eq!(Provider::Other("unknown".into()), Provider::from(String::from("")));

		assert_eq!(Provider::Other("unknown".into()), Provider::from("Unknown"));
		assert_eq!(
			Provider::Other("unknown".into()),
			Provider::from(String::from("Unknown"))
		);

		assert_eq!(Provider::Other("youtube".into()), Provider::from("Youtube"));
		assert_eq!(
			Provider::Other("youtube".into()),
			Provider::from(String::from("Youtube"))
		);

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
	fn test_clone() {
		assert_eq!(
			Provider::Other("youtube".into()),
			Provider::Other("youtube".into())
		);
	}
}
