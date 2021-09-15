use super::archive_schema::Archive;
use super::errors::GenericError;

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
	/// disable cleanup?
	pub disable_cleanup:      bool,
	/// disable re-adding the thumbnail after the editor closes
	pub disable_re_thumbnail: bool,
	/// Archive location
	pub archive:              Option<Archive>,
	/// Ask for Editing?
	pub askedit:              bool,
	/// Editor to use
	pub editor:               String,
}

#[derive(Debug)]
pub enum LineTypes {
	Youtube,
	Download,
	Ffmpeg,
	Generic,
	Unknown(String),
}

impl LineTypes {
	pub fn try_match(input: &str) -> Result<LineTypes, GenericError> {
		lazy_static! {
			static ref YTDL_OUTPUT_MATCHER: Regex = Regex::new(r"(?mi)^\s*\[(ffmpeg|download|[\w:]*)\]").unwrap();
			static ref YTDL_SELF_OUTPUT_REGEX: Regex = Regex::new(r"(?mi)^\s*Deleting\soriginal").unwrap();
		}

		if YTDL_SELF_OUTPUT_REGEX.is_match(input) {
			return Ok(LineTypes::Generic);
		}

		let cap = YTDL_OUTPUT_MATCHER
			.captures_iter(input)
			.next()
			.ok_or_else(|| return GenericError::new(format!("Coudlnt parse type for \"{}\"", input)))?;

		return Ok(match &cap[1] {
			"ffmpeg" => LineTypes::Ffmpeg,
			"download" => LineTypes::Download,
			"youtube" => LineTypes::Youtube,
			"youtube:playlist" => LineTypes::Youtube,
			"youtube:tab" => LineTypes::Youtube,
			_ => {
				info!("unknown type: {:?}", &cap[1]);
				debug!("unknown input: \"{}\"", input);
				LineTypes::Unknown(cap[1].to_string())
			},
		});
	}
}
