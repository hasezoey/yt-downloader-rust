use super::archive_schema::Archive;

use regex::Regex;
use std::path::PathBuf;

#[derive(Debug)]
/// Arguments for Youtube-DL
pub struct Arguments {
	/// Output directory
	pub out:                  PathBuf,
	/// Temporary Directory
	pub tmp:                  PathBuf,
	/// The URL to download
	pub url:                  String,
	/// Extra options passed to youtube-dl
	pub extra_args:           Vec<String>,
	/// Audio Only?
	pub audio_only:           bool,
	/// print youtube-dl stdout?
	pub debug:                bool,
	/// disable re-adding the thumbnail after the editor closes
	pub disable_re_thumbnail: bool,
	/// Archive location
	pub archive:              Option<Archive>,
	/// Ask for Editing?
	pub askedit:              bool,
	/// Editor to use
	pub editor:               String,
}

#[derive(Debug, PartialEq)]
pub enum LineTypes {
	Youtube,
	Download,
	Ffmpeg,
	Generic,
	/// Specific Information parsed from PARSE (--print), Format:
	/// Extractor, ID, Title
	Information(String, String, String),
	Unknown(String),
}

impl From<&str> for LineTypes {
	fn from(input: &str) -> Self {
		lazy_static! {
			/// Try to match for the current provider that is used by "youtube-dl"
			static ref YTDL_PROVIDER_REGEX: Regex = Regex::new(r"(?mi)\[([\w:]*)\] ").unwrap();
			/// Try to match for "youtube-dl" output itself (no provider)
			static ref YTDL_SELF_OUTPUT_REGEX: Regex = Regex::new(r"(?mi)^\s*Deleting\soriginal").unwrap();
			/// Try to match "PARSE" output (given with "--print TEMPLATE")
			static ref YTDL_PARSE_OUTPUT: Regex = Regex::new(r"(?mi)^PARSE '(.+?)' '([a-zA-Z0-9_-]{11})' (.+)$").unwrap();
		}

		if YTDL_SELF_OUTPUT_REGEX.is_match(input) {
			return Self::Generic;
		}

		if let Some(cap) = YTDL_PARSE_OUTPUT.captures_iter(input).next() {
			let mut tmpfile = cap[3].to_owned();
			tmpfile.push_str(".mp3");
			return Self::Information(cap[1].to_owned(), cap[2].to_owned(), tmpfile);
		}

		if let Some(cap) = YTDL_PROVIDER_REGEX.captures_iter(input).next() {
			return match &cap[1] {
				"ffmpeg" => Self::Ffmpeg,
				"download" => Self::Download,
				"youtube" => Self::Youtube,
				"youtube:playlist" => Self::Youtube,
				"youtube:tab" => Self::Youtube,
				"info" => Self::Generic,
				"Merger" => Self::Generic,
				"Metadata" => Self::Generic,
				"ThumbnailsConvertor" => Self::Generic,
				"EmbedThumbnail" => Self::Generic,
				_ => {
					info!("unknown type: {:?}", &cap[1]);
					debug!("unknown input: \"{}\"", input);
					LineTypes::Unknown(cap[1].to_string())
				},
			};
		}

		return LineTypes::Generic;
	}
}

#[derive(PartialEq)]
pub enum ResponseYesNo {
	Yes,
	No,
}

#[non_exhaustive]
pub enum ResponseContinue {
	Retry,
	Continue,
	Abort,
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_line_types_from() {
		assert_eq!(
			LineTypes::from("[ffmpeg] Merging formats into \"/tmp/rust-yt-dl.webm\""),
			LineTypes::Ffmpeg
		);
		assert_eq!(
			LineTypes::from("[download] Downloading playlist: test"),
			LineTypes::Download
		);
		assert_eq!(
			LineTypes::from("[youtube] someID: Downloading webpage"),
			LineTypes::Youtube
		);
		// TODO: add actual line for "youtube:tab"
		// assert_eq!(LineTypes::from("youtube:tab"), LineTypes::Youtube);
		assert_eq!(
			LineTypes::from("[youtube:playlist] playlist test: Downloading 2 videos"),
			LineTypes::Youtube
		);
		assert_eq!(
			LineTypes::from("[soundcloud] 0000000: Downloading JSON metadata"),
			LineTypes::Unknown("soundcloud".to_owned())
		);
		assert_eq!(
			LineTypes::from("Deleting original file /tmp/rust-yt-dl.f303 (pass -k to keep)"),
			LineTypes::Generic
		);
		assert_eq!(LineTypes::from("should not match"), LineTypes::Generic);
	}
}
