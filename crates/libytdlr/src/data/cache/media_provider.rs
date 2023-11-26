//! Module containing [`MediaProvider`]

use serde::{
	Deserialize,
	Serialize,
};

/// NewType struct to contain the provider in formatted form for [`super::media_info::MediaInfo`]
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct MediaProvider(String);

impl MediaProvider {
	/// Get current current [`MediaProvider`] as a str
	#[must_use]
	pub fn to_str(&self) -> &str {
		return &self.0;
	}

	/// Convert a String-like to a [`MediaProvider`]
	/// Input will be trimmed and lowercased for matching
	pub fn from_str_like<I: AsRef<str>>(input: I) -> Self {
		let mut lower = input.as_ref().trim().to_lowercase();

		if lower.is_empty() {
			lower.push_str("unknown");
		}

		return Self(lower);
	}
}

impl std::str::FromStr for MediaProvider {
	// this implementation cannot fail, because if there is no dedicated way it will fallback to variant "Other"
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		return Ok(Self::from_str_like(s));
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
		return Self::from_str_like(v);
	}
}

// Implement Display for ease-of-use
impl std::fmt::Display for MediaProvider {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// using a format! which ends up with a String, but i dont know how to circumvent that
		return write!(f, "{}", self.to_str());
	}
}

#[cfg(test)]
mod test {
	use super::*;

	mod trait_impls {
		use super::*;

		#[test]
		fn test_from_string() {
			assert_eq!(MediaProvider("youtube".to_owned()), MediaProvider::from("youtube"));
			assert_eq!(
				MediaProvider("soundcloud".to_owned()),
				MediaProvider::from("soundcloud")
			);
			assert_eq!(MediaProvider("other".to_owned()), MediaProvider::from("other"));
		}

		#[test]
		fn test_fromstr() {
			assert_eq!(Ok(MediaProvider("youtube".to_owned())), "youtube".parse());
			assert_eq!(Ok(MediaProvider("soundcloud".to_owned())), "soundcloud".parse());
			assert_eq!(Ok(MediaProvider("other".to_owned())), "other".parse());
		}

		#[test]
		fn test_as_string() {
			// reference
			assert_eq!(
				String::from("youtube"),
				String::from(&MediaProvider("youtube".to_owned()))
			);
			assert_eq!(
				String::from("soundcloud"),
				String::from(&MediaProvider("soundcloud".to_owned()))
			);
			assert_eq!(String::from("other"), String::from(&MediaProvider("other".to_owned())));

			// owned
			assert_eq!(
				String::from("youtube"),
				String::from(MediaProvider("youtube".to_owned()))
			);
			assert_eq!(
				String::from("soundcloud"),
				String::from(MediaProvider("soundcloud".to_owned()))
			);
			assert_eq!(String::from("other"), String::from(MediaProvider("other".to_owned())));
		}

		#[test]
		fn test_as_ref_str() {
			assert_eq!("youtube", MediaProvider("youtube".to_owned()).as_ref());
			assert_eq!("soundcloud", MediaProvider("soundcloud".to_owned()).as_ref());
			assert_eq!("other", MediaProvider("other".to_owned()).as_ref());
		}

		#[test]
		fn test_serialize_deserialize() {
			use serde_test::*;

			assert_tokens(
				&MediaProvider("youtube".to_owned()),
				&[Token::NewtypeStruct { name: "MediaProvider" }, Token::String("youtube")],
			);
			assert_tokens(
				&MediaProvider("soundcloud".to_owned()),
				&[
					Token::NewtypeStruct { name: "MediaProvider" },
					Token::String("soundcloud"),
				],
			);
			assert_tokens(
				&MediaProvider("other".to_owned()),
				&[Token::NewtypeStruct { name: "MediaProvider" }, Token::String("other")],
			);
		}
	}

	mod fn_impls {
		use super::*;

		#[test]
		fn test_to_str() {
			assert_eq!("youtube", MediaProvider("youtube".to_owned()).to_str());
			assert_eq!("soundcloud", MediaProvider("soundcloud".to_owned()).to_str());
			assert_eq!("other", MediaProvider("other".to_owned()).to_str());
		}

		#[test]
		fn test_from_str_like() {
			// str
			assert_eq!(
				MediaProvider("youtube".to_owned()),
				MediaProvider::from_str_like("youtube")
			);
			assert_eq!(
				MediaProvider("soundcloud".to_owned()),
				MediaProvider::from_str_like("soundcloud")
			);
			assert_eq!(MediaProvider("other".to_owned()), MediaProvider::from_str_like("other"));

			// String
			assert_eq!(
				MediaProvider("youtube".to_owned()),
				MediaProvider::from_str_like(String::from("youtube"))
			);
			assert_eq!(
				MediaProvider("soundcloud".to_owned()),
				MediaProvider::from_str_like(String::from("soundcloud"))
			);
			assert_eq!(
				MediaProvider("other".to_owned()),
				MediaProvider::from_str_like(String::from("other"))
			);
		}
	}
}
