use super::errors::GenericError;
use regex::Regex;

#[derive(Debug)]
/// Arguments for Youtube-DL
pub struct Arguments {
	/// Output directory
	pub out:        String,
	/// Temporary Directory
	pub tmp:        String,
	/// Create a Sub-Directory in the Temporary Directory?
	pub tmp_sub:    String,
	/// The URL to download
	pub url:        String,
	/// Extra options passed to youtube-dl
	pub extra_args: Vec<String>,
	/// Audio Only?
	pub audio_only: bool,
	/// print youtube-dl stdout?
	pub debug:      bool,
}

#[derive(Debug)]
pub enum YTDLOutputs {
	Youtube,
	Download,
	FFMPEG,
	Generic,
	Unkown,
}

impl YTDLOutputs {
	pub fn try_match(input: &String) -> Result<YTDLOutputs, GenericError> {
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
			"ffmpeg" => YTDLOutputs::FFMPEG,
			"download" => YTDLOutputs::Download,
			"youtube" => YTDLOutputs::Youtube,
			"youtube:playlist" => YTDLOutputs::Youtube,
			_ => {
				println!("unkown: {:?}", &cap[1]);
				YTDLOutputs::Unkown
			},
		});
	}
}
