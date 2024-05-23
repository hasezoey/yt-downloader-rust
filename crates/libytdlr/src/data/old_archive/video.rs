//! Module for the [`Video`] Struct

use serde::Deserialize;

use super::provider;

/// Struct representing a Video in the JSON archive
#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Video {
	/// The "id" of the video, as provided by "yt-dl"
	id: String,

	/// The Provider that was used
	provider: provider::Provider,

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
			file_name: String::default(),
		};
	}

	/// Builder: Set property "file_name"
	#[must_use]
	#[inline]
	pub fn with_filename<T: Into<String>>(mut self, filename: T) -> Self {
		self.set_file_name(filename);

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
}

impl From<&crate::data::sql_models::Media> for Video {
	fn from(v: &crate::data::sql_models::Media) -> Self {
		return Self {
			id:        v.media_id.clone(),
			provider:  provider::Provider::from(v.provider.clone()),
			file_name: v.title.clone(),
		};
	}
}

#[cfg(test)]
mod test {
	use crate::data::UNKNOWN;

	use super::*;

	#[test]
	fn test_new() {
		// Test basic, with &str
		assert_eq!(
			Video {
				file_name: String::new(),
				id:        String::from("helloid1"),
				provider:  provider::Provider::from(UNKNOWN),
			},
			Video::new("helloid1", provider::Provider::from(UNKNOWN))
		);

		// Test basic, with String
		assert_eq!(
			Video {
				file_name: String::new(),
				id:        String::from("helloid2"),
				provider:  provider::Provider::from(UNKNOWN),
			},
			Video::new("helloid2".to_owned(), provider::Provider::from(UNKNOWN))
		);
	}

	#[test]
	fn test_with_file_name() {
		assert_eq!(
			Video {
				file_name: String::from("hello_filename"),
				id:        String::from("helloid"),
				provider:  provider::Provider::from(UNKNOWN),
			},
			Video::new("helloid", provider::Provider::from(UNKNOWN)).with_filename("hello_filename")
		);
	}

	#[test]
	fn test_get_functions() {
		let var = Video::new("hello_id", provider::Provider::from("youtube")).with_filename("hello_file");

		assert_eq!("hello_file", var.file_name());
		assert_eq!(&provider::Provider::from("youtube"), var.provider());
		assert_eq!("hello_id", var.id());
	}

	#[test]
	fn test_set_provider() {
		let mut video1 = Video::new("id", provider::Provider::from("youtube"));
		video1.set_provider(provider::Provider::from("hello"));
		assert_eq!(Video::new("id", provider::Provider::from("hello")), video1);
	}

	#[test]
	fn test_set_file_name() {
		let mut video1 = Video::new("id", provider::Provider::from(UNKNOWN));
		video1.set_file_name("Hello");
		assert_eq!(
			Video::new("id", provider::Provider::from(UNKNOWN)).with_filename("Hello"),
			video1
		);
	}

	#[test]
	fn test_clone() {
		let video1 = Video::new("id1", provider::Provider::from("SomethingElse"));

		assert_eq!(
			Video::new("id1", provider::Provider::from("SomethingElse")),
			Video::clone(&video1)
		);
	}

	#[test]
	fn test_from_media() {
		// reference
		{
			let media = crate::data::sql_models::Media {
				_id:         0,
				media_id:    "someid".to_owned(),
				provider:    "youtube".to_owned(),
				title:       "helloTitle".to_owned(),
				inserted_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
			};

			assert_eq!(
				Video::new("someid", provider::Provider::from("youtube")).with_filename("helloTitle"),
				Video::from(&media)
			);
		}
	}
}
