use crate::unwrap_or_return;

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
use std::fmt;
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

impl Archive {
	/// convert Archive.videos to an youtube-dl archive, but only if the download was already finished
	pub fn to_ytdl_archive(&self) -> Vec<(String, &str)> {
		let mut ret = Vec::new();
		for video in &self.videos {
			if video.dl_finished {
				ret.push((String::from(&video.provider).to_lowercase(), video.id.as_ref()));
			}
		}

		return ret;
	}

	/// Add a video to the Archive (with dl_finished = false)
	pub fn add_video(&mut self, video: Video) -> () {
		// return if the id already exists in the Archive
		if let Some(fvideo) = self.videos.iter_mut().find(|v| return &v.id == &video.id) {
			// video already exists int archive.videos
			if fvideo.provider != video.provider {
				// if the providers dont match, re assign them
				match fvideo.provider {
					// assign  the new provider because the old was unkown
					Provider::Unkown => fvideo.provider = video.provider,
					// just warn that the id already exists and is *not* added to the archive
					_ => {
						warn!("Video ID \"{}\" already exists, but providers dont match! old_provider: \"{}\", new_provider: \"{}\"\n", &video.id, fvideo.provider, video.provider);
					},
				}
			}
			return;
		}
		self.videos.push(video);
	}

	/// Find the the id in the videos vec and set dl_finished to true
	pub fn mark_dl_finished(&mut self, id: &String) -> () {
		unwrap_or_return!(self.videos.iter_mut().find(|v| return &v.id == id)).dl_finished = true;
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

#[derive(Debug, PartialEq)]
pub enum Provider {
	Youtube,
	Unkown,
	Other(String),
}

impl From<&Provider> for String {
	fn from(provider: &Provider) -> Self {
		return match provider {
			Provider::Youtube => "Youtube".to_owned(),
			Provider::Unkown => "Unkown".to_owned(),
			Provider::Other(d) => format!("Other({})", d),
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
	pub fn new(id: &String, provider: Provider) -> Video {
		return Video {
			id:          id.clone(),
			provider:    provider,
			dl_finished: false,
			edit_asked:  false,
		};
	}

	/// Used to set "dl_finished" for builder
	pub fn set_dl_finished(mut self, b: bool) -> Self {
		self.dl_finished = b;

		return self;
	}
}
