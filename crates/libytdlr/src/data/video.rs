//! Module for the [`Video`] Struct

use serde::{
	Deserialize,
	Serialize,
};
use std::fmt;

use super::provider;

#[derive(Debug, Serialize, Deserialize, Default, PartialEq, Clone)]
pub struct Video {
	/// The "id" of the video, as provided by "yt-dl"
	id: String,

	/// The Provider that was used
	provider: provider::Provider,

	/// Is the video already finished downloading?
	#[serde(rename = "dlFinished", default)]
	dl_finished: bool,

	/// Was this video already asked to be edited?
	#[serde(rename = "editAsked", default)]
	edit_asked: bool,

	/// The Final File Name for the Video
	#[serde(rename = "fileName", default)]
	file_name: String,
}

impl Video {
	#[must_use]
	/// Return a new instance of "Video" with all required values and other defaults
	pub fn new<T: Into<String>>(id: T, provider: provider::Provider) -> Self {
		return Self {
			id: id.into(),
			provider,
			dl_finished: false,
			edit_asked: false,
			file_name: Default::default(),
		};
	}

	/// Builder: Set property "file_name"
	#[must_use]
	#[inline]
	pub fn with_filename<T: Into<String>>(mut self, filename: T) -> Self {
		self.set_file_name(filename);

		return self;
	}

	/// Builder: Set property "dl_finished"
	#[must_use]
	#[inline]
	pub fn with_dl_finished(mut self, to: bool) -> Self {
		self.dl_finished = to;

		return self;
	}

	/// Builder: Set property "edit_asked"
	/// If "dl_finished" is false, the property will also be set to "false"
	#[must_use]
	#[inline]
	pub fn with_edit_asked(mut self, to: bool) -> Self {
		self.set_edit_asked(to);

		return self;
	}

	/// Get Property "id"
	#[must_use]
	#[inline]
	pub fn id(&self) -> &str {
		return self.id.as_ref();
	}

	/// Get Property "file_name"
	#[must_use]
	#[inline]
	pub fn file_name(&self) -> &str {
		return self.file_name.as_ref();
	}

	/// Get Property "provider"
	#[must_use]
	#[inline]
	pub fn provider(&self) -> &provider::Provider {
		return &self.provider;
	}

	/// Get Property "dl_finished"
	#[must_use]
	#[inline]
	pub fn dl_finished(&self) -> bool {
		return self.dl_finished;
	}

	/// Get Property "edit_asked"
	#[must_use]
	#[inline]
	pub fn edit_asked(&self) -> bool {
		return self.edit_asked;
	}

	/// Set the property "dl_finished" to "to"
	#[inline]
	pub fn set_dl_finished(&mut self, to: bool) {
		self.dl_finished = to;
	}

	/// Set the property "edit_asked" to "to"
	/// If "dl_finished" is false, the property will also be set to "false"
	#[inline]
	pub fn set_edit_asked(&mut self, to: bool) {
		if !self.dl_finished {
			log::debug!("Setting \"edit_asked\" to false, because \"dl_finished\" is still \"false\"");
			self.edit_asked = false;
		} else {
			self.edit_asked = to;
		}
	}

	/// Set the property "provider" to "to"
	#[inline]
	pub fn set_provider(&mut self, to: provider::Provider) {
		self.provider = to;
	}

	/// Set the property "file_name" to "to"
	#[inline]
	pub fn set_file_name<T: Into<String>>(&mut self, to: T) {
		self.file_name = to.into();
	}

	/// Check the Video if all the options are set correctly
	/// Returns "true" if something was changed and "false" if not
	#[inline]
	pub fn check_all(&mut self) -> bool {
		let mut changed = false;

		// check that "edit_asked" is "false" when "dl_finished" is not "true"
		if !self.dl_finished && self.edit_asked {
			self.edit_asked = false;
			changed = true;
		}

		return changed;
	}

	/// Generate a [`Video`] with invalid options (like from a serde parse)
	#[cfg(test)]
	pub fn generate_invalid_options() -> Self {
		return Video {
			dl_finished: false,
			edit_asked:  true,
			file_name:   "".to_owned(),
			id:          "someID".to_owned(),
			provider:    provider::Provider::Youtube,
		};
	}
}

impl fmt::Display for Video {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		return write!(
			f,
			"Video: name: \"{}\", id: \"{}\", provider: \"{}\"",
			self.file_name, self.id, self.provider
		);
	}
}

impl From<crate::data::sql_models::Media> for Video {
	fn from(v: crate::data::sql_models::Media) -> Self {
		return Self {
			id:          v.media_id,
			provider:    provider::Provider::from(v.provider),
			dl_finished: true,
			edit_asked:  true,
			file_name:   v.title,
		};
	}
}

impl From<&crate::data::sql_models::Media> for Video {
	fn from(v: &crate::data::sql_models::Media) -> Self {
		return Self {
			id:          v.media_id.clone(),
			provider:    provider::Provider::from(v.provider.clone()),
			dl_finished: true,
			edit_asked:  true,
			file_name:   v.title.clone(),
		};
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_new() {
		// Test basic, with &str
		assert_eq!(
			Video {
				dl_finished: false,
				edit_asked:  false,
				file_name:   String::from(""),
				id:          String::from("helloid1"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid1", provider::Provider::Unknown)
		);

		// Test basic, with String
		assert_eq!(
			Video {
				dl_finished: false,
				edit_asked:  false,
				file_name:   String::from(""),
				id:          String::from("helloid2"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid2".to_owned(), provider::Provider::Unknown)
		);
	}

	#[test]
	fn test_with_file_name() {
		assert_eq!(
			Video {
				dl_finished: false,
				edit_asked:  false,
				file_name:   String::from("hello_filename"),
				id:          String::from("helloid"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid", provider::Provider::Unknown).with_filename("hello_filename")
		)
	}

	#[test]
	fn test_with_dl_finished() {
		assert_eq!(
			Video {
				dl_finished: true,
				edit_asked:  false,
				file_name:   String::from(""),
				id:          String::from("helloid"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid", provider::Provider::Unknown).with_dl_finished(true)
		)
	}

	#[test]
	fn test_with_edit_asked() {
		// test setting "edit_asked" to "true" while "dl_finsihed" is still false
		assert_eq!(
			Video {
				dl_finished: false,
				edit_asked:  false,
				file_name:   String::from(""),
				id:          String::from("helloid"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid", provider::Provider::Unknown)
				.with_dl_finished(false)
				.with_edit_asked(true)
		);

		// test setting "edit_asked" to "true" while "dl_finsihed" is also "true"
		assert_eq!(
			Video {
				dl_finished: true,
				edit_asked:  true,
				file_name:   String::from(""),
				id:          String::from("helloid"),
				provider:    provider::Provider::Unknown,
			},
			Video::new("helloid", provider::Provider::Unknown)
				.with_dl_finished(true)
				.with_edit_asked(true)
		);
	}

	#[test]
	fn test_check() {
		// test that the ".check" function works and returns the correct values

		// check that both are false and does not change anything
		let mut video0 = Video::new("someID", provider::Provider::Youtube);
		assert!(!video0.dl_finished);
		assert!(!video0.edit_asked);

		assert_eq!(false, video0.check_all());
		assert!(!video0.dl_finished);
		assert!(!video0.edit_asked);

		// check that both are true and does not change anything
		let mut video1 = Video::new("someID", provider::Provider::Youtube)
			.with_dl_finished(true)
			.with_edit_asked(true);
		assert!(video1.dl_finished);
		assert!(video1.edit_asked);

		assert_eq!(false, video1.check_all());
		assert!(video1.dl_finished);
		assert!(video1.edit_asked);

		// check that both are false and change to false
		let mut video2 = Video::generate_invalid_options();
		assert!(!video2.dl_finished);
		assert!(video2.edit_asked);

		assert_eq!(true, video2.check_all());
		assert!(!video2.dl_finished);
		assert!(!video2.edit_asked);
	}

	#[test]
	fn test_get_functions() {
		let var = Video::new("hello_id", provider::Provider::Youtube).with_filename("hello_file");

		assert_eq!(false, var.dl_finished());
		assert_eq!(false, var.edit_asked());
		assert_eq!("hello_file", var.file_name());
		assert_eq!(&provider::Provider::Youtube, var.provider());
		assert_eq!("hello_id", var.id());
	}

	#[test]
	fn test_set_dl_finised() {
		// test setting it to false, while being false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_dl_finished(false);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).dl_finished(),
			video1.dl_finished()
		);

		// test setting it to false, while being true
		let mut video1 = Video::new("id", provider::Provider::Unknown).with_dl_finished(true);
		video1.set_dl_finished(false);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).dl_finished(),
			video1.dl_finished()
		);

		// test setting it to true, while being false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_dl_finished(true);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown)
				.with_dl_finished(true)
				.dl_finished(),
			video1.dl_finished()
		);

		// test setting it to true, while being true
		let mut video1 = Video::new("id", provider::Provider::Unknown).with_dl_finished(true);
		video1.set_dl_finished(true);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown)
				.with_dl_finished(true)
				.dl_finished(),
			video1.dl_finished()
		);
	}

	#[test]
	fn test_set_edit_asked() {
		// test setting it to false, while "dl_finished" is false and "edit_asked" is false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_edit_asked(false);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).edit_asked(),
			video1.edit_asked()
		);

		// test setting it to true, while "dl_finished" is false and "edit_asked" is false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_edit_asked(true);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).edit_asked(),
			video1.edit_asked()
		);

		// test setting it to false, while "dl_finished" is true and "edit_asked" is false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_dl_finished(true);
		video1.set_edit_asked(false);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).edit_asked(),
			video1.edit_asked()
		);

		// test setting it to true, while "dl_finished" is true and "edit_asked" is false
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_dl_finished(true);
		video1.set_edit_asked(true);
		assert_eq!(
			Video::new("id", provider::Provider::Unknown)
				.with_dl_finished(true)
				.with_edit_asked(true)
				.edit_asked(),
			video1.edit_asked()
		);
	}

	#[test]
	fn test_set_provider() {
		let mut video1 = Video::new("id", provider::Provider::Youtube);
		video1.set_provider(provider::Provider::Other("hello".to_owned()));
		assert_eq!(Video::new("id", provider::Provider::Other("hello".to_owned())), video1);
	}

	#[test]
	fn test_display() {
		assert_eq!(
			String::from("Video: name: \"test_name\", id: \"test_id\", provider: \"youtube\""),
			format!(
				"{}",
				Video::new("test_id", provider::Provider::Youtube).with_filename("test_name")
			)
		);
	}

	#[test]
	fn test_set_file_name() {
		let mut video1 = Video::new("id", provider::Provider::Unknown);
		video1.set_file_name("Hello");
		assert_eq!(
			Video::new("id", provider::Provider::Unknown).with_filename("Hello"),
			video1
		);
	}

	#[test]
	fn test_clone() {
		let video1 = Video::new("id1", provider::Provider::Other("SomethingElse".to_owned()));

		assert_eq!(
			Video::new("id1", provider::Provider::Other("SomethingElse".to_owned())),
			video1.clone()
		);
	}

	#[test]
	fn test_from_media() {
		// non-reference
		{
			let media = crate::data::sql_models::Media {
				_id:         0,
				media_id:    "someid".to_owned(),
				provider:    "youtube".to_owned(),
				title:       "helloTitle".to_owned(),
				inserted_at: chrono::NaiveDateTime::from_timestamp(0, 0),
			};

			assert_eq!(
				Video::new("someid", provider::Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("helloTitle"),
				Video::from(media)
			);
		}

		// reference
		{
			let media = crate::data::sql_models::Media {
				_id:         0,
				media_id:    "someid".to_owned(),
				provider:    "youtube".to_owned(),
				title:       "helloTitle".to_owned(),
				inserted_at: chrono::NaiveDateTime::from_timestamp(0, 0),
			};

			assert_eq!(
				Video::new("someid", provider::Provider::Youtube)
					.with_dl_finished(true)
					.with_edit_asked(true)
					.with_filename("helloTitle"),
				Video::from(&media)
			);
		}
	}

	mod serde {
		use super::*;
		use serde_test::*;

		#[test]
		fn test_serialize_both() {
			assert_tokens(
				&Video::new("hello_id", provider::Provider::Unknown),
				&[
					Token::Struct { name: "Video", len: 5 },
					Token::String("id"),
					Token::String("hello_id"),
					Token::String("provider"),
					Token::String("unknown"),
					Token::String("dlFinished"),
					Token::Bool(false),
					Token::String("editAsked"),
					Token::Bool(false),
					Token::String("fileName"),
					Token::String(""),
					Token::StructEnd,
				],
			)
		}
	}
}
