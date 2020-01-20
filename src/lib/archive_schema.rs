// use chrono::{
// 	DateTime,
// 	Utc,
// };
use serde::{
	Deserialize,
	Serialize,
};
use std::path::PathBuf;

fn default_version() -> String {
	return "0.1.0".to_owned();
}

fn default_vec<T>() -> Vec<T> {
	return Vec::new();
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Archive {
	#[serde(rename = "version", default = "default_version")]
	version: String,

	#[serde(rename = "lastModified", skip)] // TODO: replace with actual date
	last_modified: String,

	#[serde(rename = "playlists", default = "default_vec")]
	playlists: Vec<Playlist>,

	#[serde(rename = "videos", default = "default_vec")]
	videos: Vec<Video>,

	#[serde(skip)]
	pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Playlist {
	#[serde(rename = "url")]
	url: String,

	#[serde(rename = "finished")]
	finished: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Video {
	#[serde(rename = "id")]
	id: String,

	#[serde(rename = "provider")]
	provider: String,

	#[serde(rename = "dlFinished")]
	dl_finished: bool,

	#[serde(rename = "editAsked")]
	edit_asked: bool,
}
