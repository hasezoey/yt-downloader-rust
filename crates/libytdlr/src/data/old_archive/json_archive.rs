//! Module for the JSON Archive

use super::Video;
use serde::Deserialize;

/// The JSON Archive for YTDL-R
#[derive(Deserialize, Debug, PartialEq)]
pub struct JSONArchive {
	/// Collection of all [`Video`]'s in the archive
	#[serde(rename = "videos", default)]
	videos: Vec<Video>,
}

impl JSONArchive {
	/// Get `self.videos` as reference
	#[must_use]
	pub fn get_videos(&self) -> &Vec<Video> {
		return self.videos.as_ref();
	}
}
