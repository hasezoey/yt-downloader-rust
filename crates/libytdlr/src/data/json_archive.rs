//! Module for the JSON Archive

use crate::data::video::Video;
use serde::{
	Deserialize,
	Serialize,
};
use std::{
	io::Write,
	path::PathBuf,
};

/// The JSON Archive for YTDL-R
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
pub struct JSONArchive {
	/// Collection of all [`Video`]'s in the archive
	#[serde(rename = "videos", default)]
	videos:   Vec<Video>,
	/// The Path this Archive is saved at
	#[serde(skip)]
	pub path: PathBuf, // TODO: remove this option when possible
}

impl JSONArchive {
	/// Write all `self.videos` to `writer` in ytdl archive format
	pub fn to_ytdl_archive<T: Write>(&self, writer: &mut T) -> std::io::Result<()> {
		for video in &self.videos {
			if video.dl_finished() {
				writeln!(writer, "{} {}", video.provider(), video.id())?;
			}
		}

		return Ok(());
	}

	/// Try to add `video` to `self.videos`
	/// Returns `true` if inserted
	/// Returns `false` if already existing (and not inserted)
	pub fn add_video(&mut self, video: Video) -> bool {
		if self.find_video_by_id(video.id()).is_some() {
			return false;
		}

		self.videos.push(video);

		return true;
	}

	/// Try to find a video by `id` and return a reference
	#[must_use]
	fn find_video_by_id<I: AsRef<str>>(&self, id: I) -> Option<&Video> {
		let id = id.as_ref();
		return self.videos.iter().find(|v| return v.id() == id);
	}

	/// Try to find a video by `id` and return a mutable reference
	#[must_use]
	fn find_video_by_id_mut<I: AsRef<str>>(&mut self, id: I) -> Option<&mut Video> {
		let id = id.as_ref();
		return self.videos.iter_mut().find(|v| return v.id() == id);
	}

	/// Set a [`Video`]'s `dl_finished` to `true`
	/// Returns `true` if successfully set to `true`
	/// Returns `false` if video did not exist
	pub fn mark_dl_finished<I: AsRef<str>>(&mut self, id: I) -> bool {
		let id = id.as_ref();
		if let Some(video) = self.find_video_by_id_mut(id) {
			video.set_dl_finished(true);

			return true;
		}

		return false;
	}

	/// Set a [`Video`]'s `filename` to `filename`
	/// Returns `true` if successfully set to `filename`
	/// Returns `false` if video did not exist
	pub fn set_filename<I: AsRef<str>, T: Into<String>>(&mut self, id: I, filename: T) -> bool {
		let id = id.as_ref();
		if let Some(video) = self.find_video_by_id_mut(id) {
			video.set_file_name(filename.into());

			return true;
		}

		return false;
	}

	/// Get `self.videos` as reference
	#[must_use]
	pub fn get_videos(&self) -> &Vec<Video> {
		return self.videos.as_ref();
	}

	/// Get `self.videos` as a mutable reference
	#[must_use]
	pub fn get_videos_mut(&mut self) -> &mut Vec<Video> {
		return &mut self.videos;
	}

	/// Run [`Video::check_all`] on all videos in `self.videos`
	/// Returns `true` if at least one video got corrected
	/// Returns `false` if no video got corrected
	pub fn check_all_videos(&mut self) -> bool {
		let mut changed = false;

		for video in self.get_videos_mut() {
			let change = video.check_all();

			changed |= change;
		}

		return changed;
	}

	/// Write the current [`JSONArchive`] instance to a writer in JSON format
	pub fn write_to_writer<W: Write>(&self, writer: &mut W) -> std::io::Result<()> {
		serde_json::to_writer_pretty(writer, self)?;

		return Ok(());
	}

	/// write current [`JSONArchive`] instance to a File at `path`
	pub fn write_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> std::io::Result<()> {
		let mut file_writer = std::fs::File::create(path.as_ref())?;

		return self.write_to_writer(&mut file_writer);
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::data::provider::Provider;

	#[test]
	fn test_add_video() {
		let id = "SomeID";
		let mut archive = JSONArchive::default();

		assert_eq!(true, archive.add_video(Video::new(id, Provider::Youtube)));
		assert_eq!(false, archive.add_video(Video::new(id, Provider::Youtube)));
		assert_eq!(false, archive.add_video(Video::new(id, Provider::Unknown)));

		assert_eq!(1, archive.get_videos().len());
	}

	#[test]
	fn test_mark_dl_finished() {
		let id = "SomeID";
		let mut archive = JSONArchive::default();

		assert_eq!(true, archive.add_video(Video::new(id, Provider::Youtube)));
		assert_eq!(1, archive.get_videos().len());
		assert_eq!(false, archive.get_videos()[0].dl_finished());

		assert_eq!(true, archive.mark_dl_finished(id));

		assert_eq!(1, archive.get_videos().len());
		assert_eq!(true, archive.get_videos()[0].dl_finished());
	}

	#[test]
	fn test_set_filename() {
		let id = "SomeID";
		let mut archive = JSONArchive::default();

		assert_eq!(true, archive.add_video(Video::new(id, Provider::Youtube)));
		assert_eq!(1, archive.get_videos().len());
		assert_eq!("", archive.get_videos()[0].file_name());

		assert_eq!(true, archive.set_filename(id, "SomeFilename"));

		assert_eq!(1, archive.get_videos().len());
		assert_eq!("SomeFilename", archive.get_videos()[0].file_name());
	}

	#[test]
	fn test_to_ytdl_archive() {
		let id0 = "SomeID0";
		let id1 = "SomeID1";
		let mut archive = JSONArchive::default();

		assert_eq!(
			true,
			archive.add_video(Video::new(id0, Provider::Youtube).with_dl_finished(true))
		);
		assert_eq!(
			true,
			archive.add_video(Video::new(id1, Provider::Other("soundcloud".to_owned())).with_dl_finished(true))
		);

		let mut target = Vec::new();

		assert!(archive.to_ytdl_archive(&mut target).is_ok());

		let string0 = String::from_utf8(target);

		assert!(string0.is_ok());

		let string0 = string0.expect("Expected assert to panic");

		assert_eq!(["youtube SomeID0\n", "soundcloud SomeID1\n"].join(""), string0);
	}

	#[test]
	fn test_write_to_writer() {
		let id0 = "SomeID0";
		let id1 = "SomeID1";
		let mut archive = JSONArchive::default();

		assert_eq!(
			true,
			archive.add_video(Video::new(id0, Provider::Youtube).with_dl_finished(true))
		);
		assert_eq!(
			true,
			archive.add_video(Video::new(id1, Provider::Other("soundcloud".to_owned())).with_dl_finished(true))
		);

		let mut target = Vec::new();

		assert!(archive.write_to_writer(&mut target).is_ok());

		let string0 = String::from_utf8(target);

		assert!(string0.is_ok());

		let string0 = string0.expect("Expected assert to panic");

		assert_eq!("{\n  \"videos\": [\n    {\n      \"id\": \"SomeID0\",\n      \"provider\": \"youtube\",\n      \"dlFinished\": true,\n      \"editAsked\": false,\n      \"fileName\": \"\"\n    },\n    {\n      \"id\": \"SomeID1\",\n      \"provider\": \"soundcloud\",\n      \"dlFinished\": true,\n      \"editAsked\": false,\n      \"fileName\": \"\"\n    }\n  ]\n}", string0);
	}

	#[test]
	fn test_check_all_videos() {
		let mut archive0 = {
			let mut archive = JSONArchive::default();
			assert!(archive.add_video(Video::new("SomeID", Provider::Youtube)));

			archive
		};

		assert_eq!(false, archive0.check_all_videos());

		let mut archive1 = {
			let mut archive = JSONArchive::default();
			assert!(archive.add_video(Video::generate_invalid_options()));

			archive
		};

		assert_eq!(true, archive1.check_all_videos());

		let mut archive2 = {
			let mut archive = JSONArchive::default();
			assert!(archive.add_video(Video::new("SomeID0", Provider::Youtube)));
			assert!(archive.add_video(Video::generate_invalid_options()));
			assert!(archive.add_video(Video::new("SomeID1", Provider::Youtube)));

			archive
		};

		assert_eq!(true, archive2.check_all_videos());
	}
}
