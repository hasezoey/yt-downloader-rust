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

#[derive(Serialize, Deserialize, Debug, PartialEq)]
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

type StringProvider = String;
type ID = str;

impl Archive {
	/// convert Archive.videos to an youtube-dl archive, but only if the download was already finished
	pub fn to_ytdl_archive(&self) -> Vec<(StringProvider, &ID)> {
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

	pub fn get_mut_videos(&mut self) -> &mut Vec<Video> {
		return &mut self.videos;
	}

	pub fn set_filename(&mut self, id: &String, filename: &String) -> () {
		unwrap_or_return!(self.videos.iter_mut().find(|v| return &v.id == id)).file_name = filename.clone();
	}
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Playlist {
	#[serde(rename = "url")]
	url: String,

	#[serde(rename = "finished")]
	pub finished: bool,
}

// impl Playlist {
// 	pub fn new(url: String) -> Self {
// 		return Playlist {
// 			url:      url,
// 			finished: false,
// 		};
// 	}
// }

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

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Video {
	#[serde(rename = "id")]
	id: String,

	#[serde(rename = "provider", default = "Provider::default")]
	provider: Provider,

	#[serde(rename = "dlFinished", default = "default_bool")]
	pub dl_finished: bool,

	#[serde(rename = "editAsked", default = "default_bool")]
	pub edit_asked: bool,

	#[serde(rename = "fileName", default = "String::default")]
	pub file_name: String,
}

impl Video {
	pub fn new(id: &String, provider: Provider) -> Video {
		return Video {
			id:          id.clone(),
			provider:    provider,
			dl_finished: false,
			edit_asked:  false,
			file_name:   String::default(),
		};
	}

	/// Used to set "dl_finished" for builder
	pub fn set_dl_finished(mut self, b: bool) -> Self {
		self.dl_finished = b;

		return self;
	}

	pub fn set_edit_asked(&mut self, b: bool) {
		self.edit_asked = b;
	}
}

impl fmt::Display for Video {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		return write!(f, "{{ id: {}, file_name: {} }}", self.id, self.file_name);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_video_set_dl_finished() {
		let mut to_assert = Video::new(&"SomeID".to_owned(), Provider::Youtube);
		to_assert.dl_finished = true;
		assert_eq!(
			Video::new(&"SomeID".to_owned(), Provider::Youtube).set_dl_finished(true),
			to_assert
		);
	}

	#[test]
	fn test_provider_try_match() {
		assert_eq!(Provider::try_match("youtube"), Provider::Youtube);
		assert_eq!(Provider::try_match(""), Provider::Unkown);
		assert_eq!(Provider::try_match("unkown"), Provider::Unkown);
		assert_eq!(
			Provider::try_match("Something Different"),
			Provider::Other("Something Different".to_lowercase())
		);
	}

	#[test]
	fn test_string_from_provider() {
		assert_eq!(String::from(&Provider::Youtube), "Youtube".to_owned());
		assert_eq!(String::from(&Provider::Unkown), "Unkown".to_owned());
		assert_eq!(
			String::from(&Provider::Other("Hello".to_owned())),
			"Other(Hello)".to_owned()
		);
	}

	#[test]
	fn test_archive_add_video() {
		let id = "SomeID".to_owned();
		let mut archive = Archive::default();
		archive.add_video(Video::new(&id, Provider::Youtube));

		let mut should_archive: Vec<Video> = Vec::new();
		should_archive.push(Video::new(&id, Provider::Youtube));

		assert_eq!(archive.videos, should_archive);
	}

	#[test]
	fn test_archive_mark_dl_finished() {
		let id = "SomeID".to_owned();
		let mut archive = Archive::default();
		archive.add_video(Video::new(&id, Provider::Youtube));
		archive.mark_dl_finished(&id);

		let mut should_archive: Vec<Video> = Vec::new();
		should_archive.push(Video::new(&id, Provider::Youtube).set_dl_finished(true));

		assert_eq!(archive.videos[0].dl_finished, true);
	}

	#[test]
	fn test_archive_to_ytdl_archive() {
		let id1 = "SomeID".to_owned();
		let id2 = "SomeSecondID".to_owned();
		let mut archive = Archive::default();
		archive.add_video(Video::new(&id1, Provider::Youtube).set_dl_finished(true));
		archive.add_video(Video::new(&id2, Provider::Unkown).set_dl_finished(true));

		let mut should_archive: Vec<(StringProvider, &ID)> = Vec::new();
		should_archive.push((String::from(&Provider::Youtube).to_lowercase(), &id1));
		should_archive.push((String::from(&Provider::Unkown).to_lowercase(), &id2));

		assert_eq!(archive.to_ytdl_archive(), should_archive);
	}
}
