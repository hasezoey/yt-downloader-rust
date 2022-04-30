//! Module containing [`MediaInfo`]

use regex::Regex;
use serde::{
	Deserialize,
	Serialize,
};
use std::path::{
	Path,
	PathBuf,
};

use super::{
	media_provider::MediaProvider,
	media_stage::MediaStage,
};
use crate::data::sql_models::InsMedia;

/// Contains Media Information, like file-name and last processed status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaInfo {
	/// The file-name of the media
	pub filename:   Option<PathBuf>,
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
	pub fn with_filename<F: AsRef<Path>>(mut self, filename: F) -> Self {
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
	pub fn set_filename<F: AsRef<Path>>(&mut self, filename: F) {
		self.filename = Some(filename.as_ref().into());
	}

	/// Set the Provider of the current [`MediaInfo`]
	pub fn set_provider(&mut self, provider: MediaProvider) {
		self.provider = Some(provider);
	}

	/// Try to create a [`MediaInfo`] instance from a filename
	/// Parsed based on the output template defined in [`crate::main::download::assemble_ytdl_command`]
	/// Only accepts a str input, not a path one
	pub fn try_from_filename<I: AsRef<str>>(filename: &I) -> Option<Self> {
		lazy_static! {
			// Regex for getting the provider,id,title from a filename (as defined in [`crate::main::download::assemble_ytdl_command`])
			static ref FROM_PATH_REGEX: Regex = Regex::new(r"(?mi)^'([^']+)'-'([^']+)'-(.+)$").unwrap();
		}

		let filename = filename.as_ref();

		let path = Path::new(&filename);

		// "file_stem" can be safely used here, because only one extension is expected
		// eg ".mkv" but not ".tar.gz"
		let filestem = path
			.file_stem()?
			// ignore all files that cannot be transformed to a str
			.to_str()?;

		let cap = FROM_PATH_REGEX.captures(filestem)?;

		return Some(
			Self::new(&cap[2])
				.with_provider(MediaProvider::from_str_like(&cap[1]))
				.with_title(&cap[3])
				.with_filename(filename),
		);
	}
}

impl From<MediaInfo> for InsMedia {
	fn from(v: MediaInfo) -> Self {
		return Self::new(
			v.id,
			v.provider
				.map_or_else(|| return "unknown (none-provided)".to_owned(), |v| return v.to_string()),
			v.title.unwrap_or_else(|| return "unknown (none-provided)".to_owned()),
		);
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
				filename:   Some(PathBuf::from("Hello")),
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

	#[test]
	fn test_into_insmedia() {
		// test with full options
		assert_eq!(
			InsMedia::new("someid", "someprovider", "sometitle"),
			MediaInfo::new("someid")
				.with_provider(MediaProvider::Other("someprovider".to_owned()))
				.with_title("sometitle")
				.into()
		);

		// test with only id
		assert_eq!(
			InsMedia::new("someid", "unknown (none-provided)", "unknown (none-provided)"),
			MediaInfo::new("someid").into()
		);
	}

	#[test]
	fn test_try_from_filename() {
		// test a non-proper name
		let input = "impropername.something";
		assert_eq!(None, MediaInfo::try_from_filename(&input));

		// test a proper name
		let input = "'provider'-'id'-Some Title.something";
		assert_eq!(
			Some(
				MediaInfo::new("id")
					.with_provider(MediaProvider::Other("provider".to_owned()))
					.with_title("Some Title")
					.with_filename("'provider'-'id'-Some Title.something")
			),
			MediaInfo::try_from_filename(&input)
		);
	}
}
