// use chrono::{
// 	DateTime,
// 	Utc,
// };
use serde::{
	Deserialize,
	Serialize,
};
use std::default::Default;
use std::path::PathBuf;

/// used for serde default
fn default_version() -> String {
	return "0.1.0".to_owned();
}

/// used for serde default
fn default_vec<T>() -> Vec<T> {
	return Vec::new();
}

/// used for serde default
fn default_last_modified() -> String {
	return "".to_owned();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Archive {
	#[serde(rename = "version", default = "default_version")]
	version: String,

	#[serde(rename = "lastModified", default = "default_last_modified")] // TODO: replace with actual date
	last_modified: String,

	#[serde(rename = "playlists", default = "default_vec")]
	playlists: Vec<Playlist>,

	#[serde(rename = "videos", default = "default_vec")]
	videos: Vec<Video>,

	#[serde(skip)]
	pub path: PathBuf,
}

impl Default for Archive {
	fn default() -> Archive {
		return Archive {
			version:       default_version(),
			last_modified: "".to_owned(),
			playlists:     default_vec(),
			videos:        default_vec(),
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

#[derive(Serialize, Deserialize, Debug)]
pub struct Video {
	#[serde(rename = "id")]
	id: String,

	#[serde(rename = "provider")]
	provider: String,

	#[serde(rename = "dlFinished")]
	pub dl_finished: bool,

	#[serde(rename = "editAsked")]
	pub edit_asked: bool,
}

impl Video {
	pub fn new(id: String, provider: String) -> Video {
		return Video {
			id:          id,
			provider:    provider,
			dl_finished: false,
			edit_asked:  false,
		};
	}
}
