// use chrono::{
// 	DateTime,
// 	Utc,
// };
use serde::{
	Deserialize,
	Deserializer,
	Serialize,
	Serializer,
	*,
};
use std::default::Default;
use std::path::PathBuf;

/// used for serde default
fn default_version() -> String {
	return "0.1.0".to_owned();
}

/// used for serde default
fn default_last_modified() -> String {
	// this is in a function for adding dates later
	return "".to_owned();
}

/// used for serde default
fn default_bool() -> bool {
	return false;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Archive {
	#[serde(rename = "version", default = "default_version")]
	version: String,

	#[serde(rename = "lastModified", default = "default_last_modified")] // TODO: replace with actual date
	last_modified: String,

	#[serde(rename = "playlists", default)]
	playlists: Vec<Playlist>,

	#[serde(rename = "videos", default)]
	videos: Vec<Video>,

	#[serde(skip)]
	pub path: PathBuf,
}

impl Default for Archive {
	fn default() -> Archive {
		return Archive {
			version:       default_version(),
			last_modified: "".to_owned(),
			playlists:     Vec::default(),
			videos:        Vec::default(),
			path:          PathBuf::from(""),
		};
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Playlist {
	#[serde(rename = "url")]
	url: String,

	#[serde(rename = "finished")]
	pub finished: bool,
}

impl Playlist {
	pub fn new(url: String) -> Self {
		return Playlist {
			url:      url,
			finished: false,
		};
	}
}

#[derive(Debug)]
pub enum Provider {
	Youtube,
	Unkown,
	Other(String),
}

impl Default for Provider {
	fn default() -> Provider {
		return Provider::Unkown;
	}
}

impl Serialize for Provider {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// match self with Provider-Variants to output correct provider
		return serializer.serialize_str(match self {
			Provider::Unkown => "",
			Provider::Other(v) => v,
			Provider::Youtube => "youtube",
		});
	}
}

impl<'de> Deserialize<'de> for Provider {
	fn deserialize<D>(deserializer: D) -> Result<Provider, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct ProviderVisitor;

		// not implementing other visit_* functions, because only str is expected
		impl<'de> de::Visitor<'de> for ProviderVisitor {
			type Value = Provider;

			// {"provider": "something"} will always result in an str
			fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
				return Ok(Provider::try_match(v));
			}

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				write!(formatter, "an String to be parsed into an Provider-Variant")?;

				return Ok(());
			}
		}

		return deserializer.deserialize_str(ProviderVisitor);
	}
}

impl Provider {
	/// Try to match "input" to the Provider-Variants
	/// if empty: Provider::Unkown
	/// if not in variants: Provider::Other(String)
	///
	/// Mainly used for Serialization and Deserialization
	pub fn try_match<I: AsRef<str>>(input: I) -> Provider {
		let finput = input.as_ref().trim().to_lowercase();

		return match finput.as_ref() {
			"youtube" => Provider::Youtube,
			"" | "unkown" => Provider::Unkown,
			_ => Provider::Other(finput),
		};
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Video {
	#[serde(rename = "id")]
	id: String,

	#[serde(rename = "provider", default = "Provider::default")]
	provider: Provider,

	#[serde(rename = "dlFinished", default = "default_bool")]
	pub dl_finished: bool,

	#[serde(rename = "editAsked", default = "default_bool")]
	pub edit_asked: bool,
}

impl Video {
	pub fn new(id: String, provider: Provider) -> Video {
		return Video {
			id:          id,
			provider:    provider,
			dl_finished: false,
			edit_asked:  false,
		};
	}
}
