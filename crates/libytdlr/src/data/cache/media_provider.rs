//! Module containing [`MediaProvider`]

/// Enum for providers that shows what provider is used for [`super::media_info::MediaInfo`]
/// Has Variants that either are common or need special handling, otherwise will result in [`MediaProvider::Other`]
#[derive(Debug, PartialEq, Clone)]
#[non_exhaustive]
pub enum MediaProvider {
	Youtube,
	Soundcloud,
	// this case is ensured to be
	Other(String),
}

impl MediaProvider {
	/// Get current current [`MediaProvider`] as a str
	pub fn to_str<'a>(&'a self) -> &'a str {
		return match self {
			MediaProvider::Youtube => "youtube",
			MediaProvider::Soundcloud => "soundcloud",
			MediaProvider::Other(v) => v,
		};
	}

	/// Convert a String-like to a [`MediaProvider`]
	/// Input will be trimmed and lowercased for matching
	pub fn from_str<I: AsRef<str>>(input: I) -> Self {
		let lower = input.as_ref().trim().to_lowercase();

		return match lower.as_str() {
			"youtube" => Self::Youtube,
			"soundcloud" => Self::Soundcloud,
			_ => Self::Other(lower),
		};
	}
}

// Implement FROM reference-Self to String
impl From<&MediaProvider> for String {
	fn from(p: &MediaProvider) -> Self {
		return p.to_str().to_owned();
	}
}

// Implement FROM Self to String
impl From<MediaProvider> for String {
	fn from(p: MediaProvider) -> Self {
		return p.to_str().to_owned();
	}
}

// Implement Casting Self as str
impl AsRef<str> for MediaProvider {
	fn as_ref(&self) -> &str {
		return self.to_str();
	}
}

// Implement FROM str to Self
impl From<&str> for MediaProvider {
	fn from(v: &str) -> Self {
		return Self::from_str(v);
	}
}

// Implement Display for ease-of-use
impl std::fmt::Display for MediaProvider {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// using a format! which ends up with a String, but i dont know how to circumvent that
		return write!(f, "{}", self.to_str());
	}
}

// Implement custom Serialize because otherwise serde would use the variant names instead of the to_string names
impl serde::Serialize for MediaProvider {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		return serializer.serialize_str(&String::from(self));
	}
}

// Implement custom Deserialize because otherwise serde would look-up the variant names instead of the from_string names
impl<'de> serde::Deserialize<'de> for MediaProvider {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct MediaProviderVisitor;

		impl<'de> serde::de::Visitor<'de> for MediaProviderVisitor {
			type Value = MediaProvider;

			// {"provider": "something"} will always result in an str
			fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
				return Ok(MediaProvider::from(v));
			}

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				write!(formatter, "a String to be parsed into a MediaProvider")?;

				return Ok(());
			}
		}

		return deserializer.deserialize_str(MediaProviderVisitor);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	mod trait_impls {
		use super::*;

		#[test]
		fn test_from_string() {
			assert_eq!(MediaProvider::Youtube, MediaProvider::from("youtube"));
			assert_eq!(MediaProvider::Soundcloud, MediaProvider::from("soundcloud"));
			assert_eq!(MediaProvider::Other("other".to_owned()), MediaProvider::from("other"));
		}

		#[test]
		fn test_as_string() {
			// reference
			assert_eq!(String::from("youtube"), String::from(&MediaProvider::Youtube));
			assert_eq!(String::from("soundcloud"), String::from(&MediaProvider::Soundcloud));
			assert_eq!(
				String::from("other"),
				String::from(&MediaProvider::Other("other".to_owned()))
			);

			// owned
			assert_eq!(String::from("youtube"), String::from(MediaProvider::Youtube));
			assert_eq!(String::from("soundcloud"), String::from(MediaProvider::Soundcloud));
			assert_eq!(
				String::from("other"),
				String::from(MediaProvider::Other("other".to_owned()))
			);
		}

		#[test]
		fn test_as_ref_str() {
			assert_eq!("youtube", MediaProvider::Youtube.as_ref());
			assert_eq!("soundcloud", MediaProvider::Soundcloud.as_ref());
			assert_eq!("other", MediaProvider::Other("other".to_owned()).as_ref());
		}

		#[test]
		fn test_serialize_deserialize() {
			use serde_test::*;
			assert_tokens(&MediaProvider::Youtube, &[Token::String("youtube")]);
			assert_tokens(&MediaProvider::Soundcloud, &[Token::String("soundcloud")]);
			assert_tokens(&MediaProvider::Other("other".to_owned()), &[Token::String("other")]);
		}
	}

	mod fn_impls {
		use super::*;

		#[test]
		fn test_to_str() {
			assert_eq!("youtube", MediaProvider::Youtube.to_str());
			assert_eq!("soundcloud", MediaProvider::Soundcloud.to_str());
			assert_eq!("other", MediaProvider::Other("other".to_owned()).to_str());
		}

		#[test]
		fn test_from_str() {
			// str
			assert_eq!(MediaProvider::Youtube, MediaProvider::from_str("youtube"));
			assert_eq!(MediaProvider::Soundcloud, MediaProvider::from_str("soundcloud"));
			assert_eq!(
				MediaProvider::Other("other".to_owned()),
				MediaProvider::from_str("other")
			);

			// String
			assert_eq!(MediaProvider::Youtube, MediaProvider::from_str("youtube".to_owned()));
			assert_eq!(
				MediaProvider::Soundcloud,
				MediaProvider::from_str("soundcloud".to_owned())
			);
			assert_eq!(
				MediaProvider::Other("other".to_owned()),
				MediaProvider::from_str("other".to_owned())
			);
		}
	}
}
