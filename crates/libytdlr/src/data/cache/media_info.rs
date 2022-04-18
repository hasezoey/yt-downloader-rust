//! Module containing [`MediaInfo`]

use serde::{
	Deserialize,
	Serialize,
};

use super::{
	media_provider::MediaProvider,
	media_stage::MediaStage,
};

/// Contains Media Information, like file-name and last processed status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
	/// The file-name of the media
	pub filename:   Option<String>,
	/// The title of the media, may differ from "filename"
	pub title:      Option<String>,
	/// The ID of the media,
	pub id:         String,
	/// The Provider that provided this media
	pub provider:   Option<MediaProvider>,
	/// The stage this media had last processed
	pub last_stage: MediaStage,
}

impl MediaInfo {
	/// Crate a new instance of [`MediaInfo`]
	pub fn new<I: AsRef<str>>(id: I) -> Self {
		return Self {
			id:         id.as_ref().into(),
			filename:   None,
			title:      None,
			last_stage: MediaStage::None,
			provider:   None,
		};
	}

	/// Builder function to add a filename
	pub fn with_filename<F: AsRef<str>>(mut self, filename: F) -> Self {
		self.filename = Some(filename.as_ref().into());

		return self;
	}

	/// Builder function to add a title
	pub fn with_title<T: AsRef<str>>(mut self, title: T) -> Self {
		self.title = Some(title.as_ref().into());

		return self;
	}

	/// Builder function to add a provider
	pub fn with_provider(mut self, provider: MediaProvider) -> Self {
		self.provider = Some(provider);

		return self;
	}

	/// Set the filename of the current [`MediaInfo`]
	pub fn set_filename<F: AsRef<str>>(&mut self, filename: F) {
		self.filename = Some(filename.as_ref().into());
	}

	/// Set the Provider of the current [`MediaInfo`]
	pub fn set_provider(&mut self, provider: MediaProvider) {
		self.provider = Some(provider);
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_new() {
		assert_eq!(
			MediaInfo {
				id:         "".to_owned(),
				filename:   None,
				title:      None,
				last_stage: MediaStage::None,
				provider:   None,
			},
			MediaInfo::new("")
		);

		assert_eq!(
			MediaInfo {
				id:         "hello".to_owned(),
				filename:   None,
				title:      None,
				last_stage: MediaStage::None,
				provider:   None,
			},
			MediaInfo::new("hello")
		);
	}

	#[test]
	fn test_with_filename() {
		assert_eq!(
			MediaInfo {
				id:         "someid".to_owned(),
				filename:   Some("Hello".to_owned()),
				title:      None,
				last_stage: MediaStage::None,
				provider:   None,
			},
			MediaInfo::new("someid").with_filename("Hello")
		);
	}

	#[test]
	fn test_with_title() {
		assert_eq!(
			MediaInfo {
				id:         "someid".to_owned(),
				filename:   None,
				title:      Some("Hello".to_owned()),
				last_stage: MediaStage::None,
				provider:   None,
			},
			MediaInfo::new("someid").with_title("Hello")
		);
	}

	#[test]
	fn test_with_provider() {
		assert_eq!(
			MediaInfo {
				id:         "someid".to_owned(),
				filename:   None,
				title:      None,
				last_stage: MediaStage::None,
				provider:   Some(MediaProvider::Youtube),
			},
			MediaInfo::new("someid").with_provider(MediaProvider::Youtube)
		);
	}
}
