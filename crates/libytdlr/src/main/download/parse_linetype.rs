use once_cell::sync::Lazy;
use regex::Regex;

use crate::data::cache::media_info::MediaInfo;

/// Helper Enum for differentiating [`LineType::Custom`] types like "PARSE_START" and "PARSE_END"
#[derive(Debug, PartialEq, Clone)]
pub enum CustomParseType {
	Start(MediaInfo),
	End(MediaInfo),
	Playlist(usize),
	Move(MediaInfo),
}

/// Line type for a ytdl output line
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LineType {
	/// Variant for FFmpeg processing lines
	Ffmpeg,
	/// Variant for ytdl download progress lines
	Download,
	/// Variant for provider specific lines (like youtube counting website)
	ProviderSpecific,
	/// Variant for generic lines (like "Deleting original file")
	Generic,
	/// Variant for lines that are from "--print"
	Custom,
	/// Variant for lines that start with "ERROR:"
	Error,
	/// Variant for lines that start with "WARNING:"
	Warning,
	/// Variant for archive skip lines
	ArchiveSkip,
}

impl LineType {
	/// Try to get the correct Variant for a input line
	/// Will return [`None`] if no type has been found
	pub fn try_from_line(input: &str) -> Option<Self> {
		/// basic regex to test if the line is "[something] something", and if it is, return what is inside "[]"
		static BASIC_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^\[([\da-z:_]*)\]").unwrap();
		});
		/// regex to check for generic lines
		static GENERIC_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^deleting original file").unwrap();
		});
		/// regex to check for skip lines
		static YTDL_ARCHIVE_SKIP_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^\[\w+\] [^:]+: has already been recorded in the archive$").unwrap();
		});
		/// regex to check for "[] Playlist ...:" lines
		static YTDL_PLAYLIST_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^\[[\w:]+\] Playlist [^:]+:").unwrap();
		});

		// check if the line is from a provider-like output
		if let Some(cap) = BASIC_TYPE_REGEX.captures(input) {
			let name = &cap[1];

			if YTDL_ARCHIVE_SKIP_REGEX.is_match(input) {
				return Some(Self::ArchiveSkip);
			}

			if YTDL_PLAYLIST_REGEX.is_match(input) {
				// this likely should have its own LineType, but for now the path of "Custom" is used
				return Some(Self::Custom);
			}

			// this case is first, because it is the most common case
			if name == "download" {
				return Some(Self::Download);
			}

			if name == "ffmpeg" {
				return Some(Self::Ffmpeg);
			}

			// everything that is not specially handled before, will get treated as being a provider
			return Some(Self::ProviderSpecific);
		}

		// matches both "PARSE_START" and "PARSE_END"
		if input.starts_with("PARSE") {
			return Some(Self::Custom);
		}

		if input.starts_with("PLAYLIST") {
			return Some(Self::Custom);
		}

		if input.starts_with("MOVE") {
			return Some(Self::Custom);
		}

		// check for Generic lines that dont have a prefix
		if GENERIC_TYPE_REGEX.is_match(input) {
			return Some(Self::Generic);
		}

		if input.starts_with("ERROR:") {
			return Some(Self::Error);
		}

		if input.starts_with("youtube-dl: error:") {
			return Some(Self::Error);
		}

		if input.starts_with("WARNING:") {
			return Some(Self::Warning);
		}

		// if nothing above matches, return None, because no type has been found
		return None;
	}

	/// Try to get the download precent from input
	/// Returns [`None`] if not being of variant [`LineType::Download`] or if not percentage can be found or could not be parsed
	pub fn try_get_download_percent<I: AsRef<str>>(&self, input: I) -> Option<u8> {
		// this function only works with Download lines
		if self != &Self::Download {
			return None;
		}

		/// Regex to parse the download percentage from a line
		/// cap1: precentage(not decimal)
		static DOWNLOAD_PERCENTAGE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^\[download\]\s+(\d{1,3})(?:\.\d)?%").unwrap();
		});

		let input = input.as_ref();

		if let Some(cap) = DOWNLOAD_PERCENTAGE_REGEX.captures(input) {
			let percent_str = &cap[1];

			// directly use the "Result" returned by "from_str_radix" and convert it to a "Option"
			return percent_str.parse::<u8>().ok();
		}

		return None;
	}

	/// Try to parse the custom parse-helpers like "PARSE_START"
	/// Retruns [`None`] if not being of variant [`LineType::Custom`] or if no parse helper can be found
	pub fn try_get_parse_helper<I: AsRef<str>>(&self, input: I) -> Option<CustomParseType> {
		// this function only works with Custom lines
		if self != &Self::Custom {
			return None;
		}

		/// Regex to get all information from the Parsing helper "PARSE_START" and "PARSE_END"
		static PARSE_START_END_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^PARSE_(START|END) '([^']+)' '([^']+)'(?: (.+))?$").unwrap();
		});
		/// Regex to get all information from the Parsing helper "PLAYLIST"
		static PARSE_PLAYLIST_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^PLAYLIST '(\d+)'$").unwrap();
		});
		/// Regex to get all information from the Parsing helper "MOVE"
		static PARSE_MOVE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^MOVE '([^']+)' '([^']+)' (.+)$").unwrap();
		});
		/// regex to check for "[] Playlist ...: Downloading ... items of ..." lines
		static YTDL_PLAYLIST_COUNT_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^\[[\w:]+\] Playlist [^:]+: Downloading (\d+) items of (\d+)$").unwrap();
		});

		let input = input.as_ref();

		// handle "PARSE_START" and "PARSE_END" lines
		if let Some(cap) = PARSE_START_END_REGEX.captures(input) {
			let line_type = &cap[1];
			let provider = &cap[2];
			let id = &cap[3];

			match line_type {
				"START" => {
					let title = &cap[4];

					return Some(CustomParseType::Start(MediaInfo::new(id, provider).with_title(title)));
				},
				"END" => {
					return Some(CustomParseType::End(MediaInfo::new(id, provider)));
				},
				// the following is unreachable, because the Regex ensures that only "START" and "END" match
				_ => unreachable!(),
			}
		}

		// handle "MOVE" lines
		// cannot be merged easily with "PARSE_END", because of https://github.com/yt-dlp/yt-dlp/issues/7197#issuecomment-1572066439
		if let Some(cap) = PARSE_MOVE_REGEX.captures(input) {
			let provider = &cap[1];
			let id = &cap[2];
			let file_path = std::path::PathBuf::from(&cap[3]);

			let Some(filename) = file_path.file_name() else {
				info!("MOVE path from youtube-dl did not have a file_name!");
				return None;
			};

			return Some(CustomParseType::Move(
				MediaInfo::new(id, provider).with_filename(filename),
			));
		}

		// handle "[] Playlist ...: Downloading ... items of ..." lines
		if let Some(cap) = YTDL_PLAYLIST_COUNT_REGEX.captures(input) {
			let count_str = &cap[1];

			return match count_str.parse::<usize>() {
				Ok(count) => Some(CustomParseType::Playlist(count)),
				Err(err) => {
					info!("Failed to parse \"[] Playlist ...: Downloading ... items of ...\" count, error: {err}");
					None
				},
			};
		}

		// handle "PLAYLIST" lines
		if let Some(cap) = PARSE_PLAYLIST_REGEX.captures(input) {
			let count_str = &cap[1];

			return match count_str.parse::<usize>() {
				Ok(count) => Some(CustomParseType::Playlist(count)),
				Err(err) => {
					info!("Failed to parse PLAYLIST count, error: {err}");
					None
				},
			};
		}

		return None;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_try_from_line() {
		let input = "[download] Downloading playlist: test";
		assert_eq!(Some(LineType::Download), LineType::try_from_line(input));

		let input = "[download]   0.0% of 51.32MiB at 160.90KiB/s ETA 05:29";
		assert_eq!(Some(LineType::Download), LineType::try_from_line(input));

		let input = "[youtube:playlist] playlist test: Downloading 2 videos";
		assert_eq!(Some(LineType::ProviderSpecific), LineType::try_from_line(input));

		let input = "[youtube] -----------: Downloading webpage";
		assert_eq!(Some(LineType::ProviderSpecific), LineType::try_from_line(input));

		let input = "[ffmpeg] Merging formats into \"/tmp/rust-yt-dl.webm\"";
		assert_eq!(Some(LineType::Ffmpeg), LineType::try_from_line(input));

		let input = "Deleting original file /tmp/rust-yt-dl.f303 (pass -k to keep)";
		assert_eq!(Some(LineType::Generic), LineType::try_from_line(input));

		let input = "Something unexpected";
		assert_eq!(None, LineType::try_from_line(input));

		let input = "PARSE_START 'youtube' '-----------' Some Title Here";
		assert_eq!(Some(LineType::Custom), LineType::try_from_line(input));

		let input = "PARSE_END 'youtube' '-----------'";
		assert_eq!(Some(LineType::Custom), LineType::try_from_line(input));

		let input = "ERROR: [provider] id: Unable to download webpage: The read operation timed out";
		assert_eq!(Some(LineType::Error), LineType::try_from_line(input));

		let input = r#"youtube-dl: error: invalid thumbnail format ""webp>jpg"" given"#;
		assert_eq!(Some(LineType::Error), LineType::try_from_line(input));

		let input = "WARNING: [youtube] Falling back to generic n function search
         player = https://somewhere.com/some.js";
		assert_eq!(Some(LineType::Warning), LineType::try_from_line(input));

		let input = "[download] someid: has already been recorded in the archive";
		assert_eq!(Some(LineType::ArchiveSkip), LineType::try_from_line(input));
	}

	#[test]
	fn test_linetype_download_unknown() {
		let input = "[download]   0.0% of   75.34MiB at  Unknown B/s ETA Unknown";

		let linetype = LineType::try_from_line(input).unwrap();
		assert_eq!(LineType::Download, linetype);
		assert_eq!(Some(0), linetype.try_get_download_percent(input));
	}

	#[test]
	fn test_try_get_download_percent() {
		// should try to apply the regex, but would not find anything
		let input = "[download] Downloading playlist: test";
		assert_eq!(None, LineType::Download.try_get_download_percent(input));

		// should find "0"
		let input = "[download]   0.0% of 51.32MiB at 160.90KiB/s ETA 05:29";
		assert_eq!(Some(0), LineType::Download.try_get_download_percent(input));

		// should find "1"
		let input = "[download]   1.0% of  290.41MiB at  562.77KiB/s ETA 08:43";
		assert_eq!(Some(1), LineType::Download.try_get_download_percent(input));

		// should find "1"
		let input = "[download]   1.1% of  290.41MiB at  568.08KiB/s ETA 08:37";
		assert_eq!(Some(1), LineType::Download.try_get_download_percent(input));

		// should find "75"
		let input = "[download]  75.6% of 51.32MiB at  2.32MiB/s ETA 00:05";
		assert_eq!(Some(75), LineType::Download.try_get_download_percent(input));

		// should find "100"
		let input = "[download] 100% of 2.16MiB in 00:00";
		assert_eq!(Some(100), LineType::Download.try_get_download_percent(input));

		// should early-return because not correct variant
		let input = "something else";
		assert_eq!(None, LineType::Generic.try_get_download_percent(input));

		// test out-of-u8-bounds
		let input = "[download] 256% of 2.16MiB in 00:00";
		assert_eq!(None, LineType::Download.try_get_download_percent(input));
	}

	#[test]
	fn test_try_get_parse_helper() {
		// should early-return because of not being the correct variant
		let input = "[download] Downloading playlist: test";
		assert_eq!(None, LineType::Download.try_get_parse_helper(input));

		// should find PARSE_START and get "provider, id, title"
		let input = "PARSE_START 'youtube' '-----------' Some Title Here";
		assert_eq!(
			Some(CustomParseType::Start(
				MediaInfo::new("-----------", "youtube").with_title("Some Title Here")
			)),
			LineType::Custom.try_get_parse_helper(input)
		);

		// should find "PARSE_END" and get "provider, id"
		let input = "PARSE_END 'youtube' '-----------'";
		assert_eq!(
			Some(CustomParseType::End(MediaInfo::new("-----------", "youtube"))),
			LineType::Custom.try_get_parse_helper(input)
		);

		// should not match the regex
		let input = "PARSE";
		assert_eq!(None, LineType::Custom.try_get_parse_helper(input));

		// should return because of not matching the regex
		let input = "Something Unexpected";
		assert_eq!(None, LineType::Custom.try_get_parse_helper(input));
	}
}
