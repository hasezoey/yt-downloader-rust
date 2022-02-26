use crate::unwrap_or_return;

use serde::{
	Deserialize,
	Serialize,
};
use std::io::{
	Error as ioError,
	Write,
};
use std::path::PathBuf;

use crate::data::video::Video;

/// used for serde default
fn default_version() -> String {
	return "0.1.0".to_owned();
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Archive {
	#[serde(rename = "version", default = "default_version")]
	version:  String,
	#[serde(rename = "videos", default)]
	videos:   Vec<Video>,
	#[serde(skip)]
	pub path: PathBuf,
}

impl std::default::Default for Archive {
	fn default() -> Archive {
		return Archive {
			version: default_version(),
			videos:  Vec::default(),
			path:    PathBuf::from(""),
		};
	}
}

impl Archive {
	/// convert Archive.videos to an youtube-dl archive, but only if the download was already finished
	pub fn to_ytdl_archive(&self) -> Vec<(String, &str)> {
		let mut ret = Vec::new();
		for video in &self.videos {
			if video.dl_finished() {
				ret.push((String::from(video.provider()).to_lowercase(), video.id()));
			}
		}

		return ret;
	}

	/// Add a video to the Archive (with dl_finished = false)
	/// Returns `true` if `video` was successfully inserted
	/// Returns `false` if `video` was no inserted
	pub fn add_video(&mut self, video: Video) -> bool {
		// return if the id already exists in the Archive
		// "avideo" = Archive Video
		if let Some(_avideo) = self.videos.iter_mut().find(|v| return v.id() == video.id()) {
			// video already exists in archive.videos
			return false;
		}
		self.videos.push(video);

		return true;
	}

	/// Find the the id in the videos vec and set dl_finished to true
	pub fn mark_dl_finished(&mut self, id: &str) {
		unwrap_or_return!(self.videos.iter_mut().find(|v| return v.id() == id)).set_dl_finished(true);
	}

	pub fn get_mut_videos(&mut self) -> &mut Vec<Video> {
		return &mut self.videos;
	}

	pub fn set_filename<T: Into<String>>(&mut self, id: &str, filename: T) {
		unwrap_or_return!(self.videos.iter_mut().find(|v| return v.id() == id)).set_file_name(filename.into());
	}

	pub fn get_videos(&self) -> &Vec<Video> {
		return self.videos.as_ref();
	}

	/// Run [`Video::check_all`] on each video
	/// Returns "true" if at least one check returned "true", otherwise "false"
	pub fn check_all_videos(&mut self) -> bool {
		let mut changed = false;

		for video in self.get_mut_videos() {
			let change = video.check_all();

			if !changed && change {
				changed = true;
			}
		}

		return changed;
	}

	/// Write the current Archive Instance in JSON from to the writer
	pub fn write_to_writer<W: Write>(&self, writer: &mut W) -> Result<(), ioError> {
		serde_json::to_writer_pretty(writer, self)?;

		return Ok(());
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::data::provider::Provider;

	#[test]
	fn test_archive_add_video() {
		let id = "SomeID".to_owned();
		let mut archive = Archive::default();
		assert_eq!(true, archive.add_video(Video::new(&id, Provider::Youtube)));

		let mut should_archive: Vec<Video> = Vec::new();
		should_archive.push(Video::new(&id, Provider::Youtube));

		assert_eq!(archive.videos, should_archive);

		assert_eq!(false, archive.add_video(Video::new(&id, Provider::Unknown)));

		assert_eq!(archive.videos, should_archive);
	}

	#[test]
	fn test_archive_mark_dl_finished() {
		let id = "SomeID".to_owned();
		let mut archive = Archive::default();
		archive.add_video(Video::new(&id, Provider::Youtube));
		archive.mark_dl_finished(&id);

		let mut should_archive: Vec<Video> = Vec::new();
		should_archive.push(Video::new(&id, Provider::Youtube).with_dl_finished(true));

		assert_eq!(archive.videos[0].dl_finished(), true);
	}

	#[test]
	fn test_archive_to_ytdl_archive() {
		let id1 = "SomeID".to_owned();
		let id2 = "SomeSecondID".to_owned();
		let mut archive = Archive::default();
		archive.add_video(Video::new(&id1, Provider::Youtube).with_dl_finished(true));
		archive.add_video(Video::new(&id2, Provider::Unknown).with_dl_finished(true));

		let mut should_archive: Vec<(String, &str)> = Vec::new();
		should_archive.push((String::from(&Provider::Youtube).to_lowercase(), &id1));
		should_archive.push((String::from(&Provider::Unknown).to_lowercase(), &id2));

		assert_eq!(archive.to_ytdl_archive(), should_archive);
	}

	#[test]
	fn test_check_video_all() {
		// test that "check_all_videos" returns the correct value

		let mut archive0 = {
			let mut archive = Archive::default();
			archive.add_video(Video::new("someID", Provider::Youtube));

			archive
		};

		assert_eq!(false, archive0.check_all_videos());

		let mut archive1 = {
			let mut archive = Archive::default();
			archive.add_video(Video::generate_invalid_options());

			archive
		};

		assert_eq!(true, archive1.check_all_videos());

		let mut archive2 = {
			let mut archive = Archive::default();
			archive.add_video(Video::new("someID1", Provider::Youtube));
			archive.add_video(Video::generate_invalid_options());
			archive.add_video(Video::new("someID2", Provider::Youtube));

			archive
		};

		assert_eq!(true, archive2.check_all_videos());
	}

	#[test]
	fn test_write_to_writer() {
		let mut writer = Vec::new();

		let archive = Archive::default();

		let result = archive.write_to_writer(&mut writer);

		assert!(result.is_ok());

		assert_eq!(
			["{\n", "  \"version\": \"0.1.0\",\n", "  \"videos\": []\n", "}"].concat(),
			String::from_utf8_lossy(&writer)
		);
	}
}
