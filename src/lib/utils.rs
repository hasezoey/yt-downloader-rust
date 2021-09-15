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
pub enum YTDLOutputs {
	Youtube,
	Download,
	Ffmpeg,
	Generic,
	Unknown(String),
}

impl YTDLOutputs {
	pub fn try_match(input: &str) -> Result<YTDLOutputs, GenericError> {
		lazy_static! {
			static ref YTDL_OUTPUT_MATCHER: Regex = Regex::new(r"(?mi)^\s*\[(ffmpeg|download|[\w:]*)\]").unwrap();
			static ref YTDL_OUTPUT_GENERIC: Regex = Regex::new(r"(?mi)^\s*Deleting\soriginal").unwrap();
		}

		if YTDL_OUTPUT_GENERIC.is_match(input) {
			return Ok(YTDLOutputs::Generic);
		}

		let cap = YTDL_OUTPUT_MATCHER
			.captures_iter(input)
			.next()
			.ok_or_else(|| return GenericError::new(format!("Coudlnt parse type for \"{}\"", input)))?;

		return Ok(match &cap[1] {
			"ffmpeg" => YTDLOutputs::Ffmpeg,
			"download" => YTDLOutputs::Download,
			"youtube" => YTDLOutputs::Youtube,
			"youtube:playlist" => YTDLOutputs::Youtube,
			"youtube:tab" => YTDLOutputs::Youtube,
			_ => {
				info!("unknown type: {:?}", &cap[1]);
				debug!("unknown input: \"{}\"", input);
				YTDLOutputs::Unknown(cap[1].to_string())
			},
		});
	}
}
