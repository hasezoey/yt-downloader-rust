//! Module for handling youtube-dl

use diesel::SqliteConnection;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
	ffi::OsString,
	fs::File,
	io::{
		BufRead,
		BufReader,
		BufWriter,
		Write,
	},
	time::Duration,
};

use crate::{
	data::cache::media_info::MediaInfo,
	error::IOErrorToError,
	spawn::ytdl::YTDL_BIN_NAME,
	traits::download_options::DownloadOptions,
};

/// Enum for hooks to know what is currently happening
/// All Variants will have a certian order in which they are called (like AllStarting is always before a SingleStarting)
/// but not all may be called, like there may be "SingleStarting -> SingleProgress -> Skipped" instead of "SingleStarting -> SingleProgress -> SingleFinished"
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadProgress {
	/// Variant representing that the download is starting
	AllStarting,
	/// Variant representing a skipped element, may or may not come because of it already being in the archive
	/// may be called after "SingleStarting" and / or "SingleProcess" instead of "SingleFinished"
	/// values (skipped_count)
	Skipped(usize),
	/// Variant representing that a media has started the process
	/// values: (id, title)
	SingleStarting(String, String),
	/// Variant representing that a started media has increased in progress
	/// "id" may be [`None`] when the previous parsing did not parse a title
	/// values:  (id, progress)
	SingleProgress(Option<String>, u8),
	/// Variant representing that a media has finished the process
	/// the "id" is not guranteed to be the same as in [`DownloadProgress::SingleStarting`]
	/// will only be called if there was a download AND no error happened
	/// values: (id)
	SingleFinished(String),
	/// Variant representing that the download has finished
	/// The value in this tuple is the size of actually downloaded media, not just found media
	/// values: (downloaded media count)
	AllFinished(usize),
	/// Variant representing that playlist info has been found - may not trigger if not in a playlist
	/// the first (and currently only) value is the count of media in the playlist
	/// values: (playlist_count)
	PlaylistInfo(usize),
}

/// Download a single URL
/// Assumes ytdl and ffmpeg have already been checked to exist and work (like using [`crate::spawn::ytdl::ytdl_version`])
/// Adds all non-skipped Media to the input [`Vec<MediaInfo>`]
pub fn download_single<A: DownloadOptions, C: FnMut(DownloadProgress)>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
	pgcb: C,
	mediainfo_vec: &mut Vec<MediaInfo>,
) -> Result<(), crate::Error> {
	let ytdl_child = {
		let args = assemble_ytdl_command(connection, options)?;

		// merge stderr into stdout
		duct::cmd(YTDL_BIN_NAME, args)
			.stderr_to_stdout()
			.reader()
			.attach_location_err("duct ytdl reader")?
	};

	let stdout_reader = BufReader::new(&ytdl_child);

	handle_stdout(options, pgcb, stdout_reader, mediainfo_vec)?;

	loop {
		// wait loop, because somehow a "ReaderHandle" does not implement "wait", only "try_wait", but have to wait for it to exit here
		match ytdl_child.try_wait() {
			Ok(v) => {
				// only in the "Some" case is the wait actually finished
				if v.is_some() {
					break;
				}
			},
			Err(err) => {
				// ignore duct errors as non-"Err" worthy
				warn!("youtube-dl exited with a non-0 code: {err}");
				break;
			},
		}

		std::thread::sleep(Duration::from_millis(100)); // sleep to same some time between the next wait (to not cause constant cpu spike)
	}

	return Ok(());
}

/// Internal Struct for easily adding various types that resolve to [`OsString`] and output a [`Vec<OsString>`]
/// exists because [std::process::Command] is too overkill to use for a argument collection for having to use [duct] later
#[derive(Debug)]
struct ArgsHelper(Vec<OsString>);
impl ArgsHelper {
	/// Create a new instance of ArgsHelper
	pub fn new() -> Self {
		return Self(Vec::default());
	}

	/// Add a new Argument to the list, added at the end and converted to a [`OsString`]
	/// Returns the input reference to "self" for chaining
	pub fn arg<U>(&mut self, arg: U) -> &mut Self
	where
		U: Into<OsString>,
	{
		self.0.push(arg.into());

		return self;
	}

	/// Convert Self to the inner value
	/// Consumes self
	pub fn into_inner(self) -> Vec<OsString> {
		return self.0;
	}
}

impl From<ArgsHelper> for Vec<OsString> {
	fn from(v: ArgsHelper) -> Self {
		return v.into_inner();
	}
}

/// Youtube-DL archive prefix
pub const YTDL_ARCHIVE_PREFIX: &str = "ytdl_archive_";
/// Youtube-DL archive extension
pub const YTDL_ARCHIVE_EXT: &str = ".txt";

/// Consistent way of getting the archive name
#[must_use]
pub fn get_archive_name(output_dir: &std::path::Path) -> std::path::PathBuf {
	return output_dir.join(format!(
		"{}{}{}",
		YTDL_ARCHIVE_PREFIX,
		std::process::id(),
		YTDL_ARCHIVE_EXT
	));
}

/// Helper Function to assemble all ytdl command arguments
/// Returns a list of arguments for youtube-dl in order
#[inline]
fn assemble_ytdl_command<A: DownloadOptions>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
) -> Result<Vec<OsString>, crate::Error> {
	let mut ytdl_args = ArgsHelper::new();

	let output_dir = options.download_path();
	debug!("YTDL Output dir is \"{}\"", output_dir.to_string_lossy());

	std::fs::create_dir_all(output_dir).attach_path_err(output_dir)?;

	// set a custom format the videos will be in for consistent parsing
	let output_format = output_dir.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s");

	if let Some(connection) = connection {
		debug!("Found connection, generating archive");
		if let Some(archive_lines) = options.gen_archive(connection) {
			let archive_file_path = get_archive_name(output_dir);

			// write all lines to the file and drop the handle before giving the argument
			{
				let mut archive_write_handle =
					BufWriter::new(File::create(&archive_file_path).attach_path_err(&archive_file_path)?);

				for archive_line in archive_lines {
					archive_write_handle
						.write_all(archive_line.as_bytes())
						.attach_path_err(&archive_file_path)?;
				}
			}

			ytdl_args.arg("--download-archive").arg(&archive_file_path);
		}
	}

	// using unwrap, because it is checked via tests that this statement compiles and is meant to be static
	// 2023.3.24 is the date of the commit that added "--no-quiet"
	if options.ytdl_version() >= chrono::NaiveDate::from_ymd_opt(2023, 3, 24).unwrap() {
		// required to get messages about when a element is skipped because of the archive
		ytdl_args.arg("--no-quiet"); // requires a yet unreleased version of yt-dlp (higher than 2023.03.04)
	}

	// apply options to make output audio-only
	if options.audio_only() {
		// set the format that should be downloaded
		ytdl_args.arg("-f").arg("bestaudio/best");
		// set ytdl to always extract the audio, if it is not already audio-only
		ytdl_args.arg("-x");
		// set the output audio format
		ytdl_args.arg("--audio-format").arg("mp3");
	} else {
		// set the format that should be downloaded
		ytdl_args.arg("-f").arg("bestvideo+bestaudio/best");
		// set final consistent output format
		ytdl_args.arg("--remux-video").arg("mkv");
	}

	// embed the videoo thumbnail if available into the output container
	ytdl_args.arg("--embed-thumbnail");

	// add metadata to the container if the container supports it
	ytdl_args.arg("--add-metadata");

	// the following is mainly because of https://github.com/yt-dlp/yt-dlp/issues/4227
	ytdl_args.arg("--convert-thumbnails").arg("webp>jpg"); // convert webp thumbnails to jpg

	// write the media's thumbnail as a seperate file
	ytdl_args.arg("--write-thumbnail");

	if let Some(sub_langs) = options.sub_langs() {
		// add subtitles directly into the downloaded file - if available
		ytdl_args.arg("--embed-subs");

		// write subtiles as a separate file
		ytdl_args.arg("--write-subs");

		// set which subtitles to download
		ytdl_args.arg("--sub-langs").arg(sub_langs);

		// set subtitle stream as default directly in the ytdl post-processing
		ytdl_args.arg("--ppa").arg("EmbedSubtitle:-disposition:s:0 default"); // set stream 0 as default
	}

	// set custom ytdl logging for easy parsing
	{
		// print playlist information when available
		// TODO: replace with "before_playlist" once available, see https://github.com/yt-dlp/yt-dlp/issues/7034
		ytdl_args
			.arg("--print")
			// print the playlist count to get a sizehint
			.arg("before_dl:PLAYLIST '%(playlist_count)s'");

		// print once before the video starts to download to get all information and to get a consistent start point
		ytdl_args
			.arg("--print")
			.arg("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s");
		// print once after the video got fully processed to get a consistent end point
		ytdl_args
			.arg("--print")
			// only "extractor" and "id" is required, because it can be safely assumed that when this is printed, the "PARSE_START" was also printed
			.arg("after_video:PARSE_END '%(extractor)s' '%(id)s'");

		// print after move to get the filepath of the final output file
		ytdl_args
			.arg("--print")
			// includes "extractor" and "id" for identifying which media the filepath is for
			.arg("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s");
	}

	// ensure ytdl is printing progress reports
	ytdl_args.arg("--progress");
	// ensure ytdl prints the progress reports on a new line
	ytdl_args.arg("--newline");

	// ensure it is not in simulate mode (for example set via extra arguments)
	ytdl_args.arg("--no-simulate");

	// set the output directory for ytdl
	ytdl_args.arg("-o").arg(output_format);

	// apply all extra arguments
	for extra_arg in &options.extra_ytdl_arguments() {
		ytdl_args.arg(extra_arg);
	}

	// apply the url to download as the last argument
	ytdl_args.arg(options.get_url());

	return Ok(ytdl_args.into());
}

/// Helper Enum for differentiating [`LineType::Custom`] types like "PARSE_START" and "PARSE_END"
#[derive(Debug, PartialEq, Clone)]
enum CustomParseType {
	Start(MediaInfo),
	End(MediaInfo),
	Playlist(usize),
	Move(MediaInfo),
}

/// Line type for a ytdl output line
#[derive(Debug, PartialEq, Clone)]
enum LineType {
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
	/// Variant for archive skip lines
	Skip,
}

impl LineType {
	/// Try to get the correct Variant for a input line
	/// Will return [`None`] if no type has been found
	pub fn try_from_line<I: AsRef<str>>(input: I) -> Option<Self> {
		/// basic regex to test if the line is "[something] something", and if it is, return what is inside "[]"
		static BASIC_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^\[([\da-z:_]*)\]").unwrap();
		});
		/// regex to check for generic lines
		static GENERIC_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^deleting original file").unwrap();
		});
		/// regex to check for "ERROR:" lines
		static ERROR_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^ERROR:").unwrap();
		});
		/// regex to check for "youtube-dl: error:" lines
		static YTDL_ERROR_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^youtube-dl: error:").unwrap();
		});
		/// regex to check for "youtube-dl: error:" lines
		static YTDL_ARCHIVE_SKIP_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?m)^\[\w+\] [^:]+: has already been recorded in the archive$").unwrap();
		});

		let input = input.as_ref();

		// check if the line is from a provider-like output
		if let Some(cap) = BASIC_TYPE_REGEX.captures(input) {
			let name = &cap[1];
			// this case is first, because it is the most common case
			if name == "download" {
				return Some(Self::Download);
			}

			if name == "ffmpeg" {
				return Some(Self::Ffmpeg);
			}

			if YTDL_ARCHIVE_SKIP_REGEX.is_match(input) {
				return Some(Self::Skip);
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

		if ERROR_TYPE_REGEX.is_match(input) {
			return Some(Self::Error);
		}

		if YTDL_ERROR_TYPE_REGEX.is_match(input) {
			return Some(Self::Error);
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
			return Regex::new(r"(?mi)^^\[[\da-z:_]*\]\s+(\d{1,3})(?:\.\d)?%").unwrap();
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
			return Regex::new(r"(?mi)^PLAYLIST '([^']+)'$").unwrap();
		});
		/// Regex to get all information from the Parsing helper "MOVE"
		static PARSE_MOVE_REGEX: Lazy<Regex> = Lazy::new(|| {
			return Regex::new(r"(?mi)^MOVE '([^']+)' '([^']+)' (.+)$").unwrap();
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

			let filename = if let Some(name) = file_path.file_name() {
				name
			} else {
				info!("MOVE path from youtube-dl did not have a file_name!");
				return None;
			};

			return Some(CustomParseType::Move(
				MediaInfo::new(id, provider).with_filename(filename),
			));
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

/// Helper function to handle the output from a spawned ytdl command
/// Adds all non-skipped Media to the input [`Vec<MediaInfo>`]
#[inline]
fn handle_stdout<A: DownloadOptions, C: FnMut(DownloadProgress), R: BufRead>(
	options: &A,
	mut pgcb: C,
	reader: R,
	mediainfo_vec: &mut Vec<MediaInfo>,
) -> Result<(), crate::Error> {
	// report that the downloading is now starting
	pgcb(DownloadProgress::AllStarting);

	// cache the bool for "print_command_stdout" to not execute the function for every line (should be a static value)
	let print_stdout = options.print_command_stdout();

	// the array where finished "current_mediainfo" gets appended to
	// for performance / allocation efficiency, a count is requested from options
	// let mut mediainfo_vec: Vec<MediaInfo> = Vec::with_capacity(options.get_count_estimate());
	// "current_mediainfo" may not be defined because it cannot be guranteed that a parsed output was emitted
	let mut current_mediainfo: Option<MediaInfo> = None;
	// value to determine if a media has actually been downloaded, or just found
	let mut had_download = false;
	// store the last error line encountered
	let mut last_error = None;

	// HACK: .lines() iter never exits on non-0 exit codes in duct, see https://github.com/oconnor663/duct.rs/issues/112
	for line in reader.lines() {
		let line = match line {
			Ok(v) => v,
			Err(err) => {
				debug!("duct lines reader errored: {}", err);
				break; // handle it as a non-breaking case, because in 99% of cases it is just a error of "command ... exited with code ?"
			},
		};

		// only print STDOUT to output when requested
		if print_stdout {
			trace!("ytdl [STDOUT]: \"{}\"", line);
		}

		if let Some(linetype) = LineType::try_from_line(&line) {
			// clear last_error line once the linetype is not error anymore (like in playlist to not fail if the playlist is not just skipped / private media)
			if linetype != LineType::Error {
				last_error = None;
			}
			match linetype {
				// currently there is nothing that needs to be done with "Ffmpeg" lines
				LineType::Ffmpeg => (),
				// currently there is nothing that needs to be done with "ProviderSpecific" Lines, thanks to "--print"
				LineType::ProviderSpecific => (),
				// currently there is nothing that needs to be done with "Generic" Lines
				LineType::Generic => (),
				LineType::Download => {
					had_download = true;
					if let Some(percent) = linetype.try_get_download_percent(line) {
						// convert "current_mediainfo" to a reference and operate on the inner value (if exists) to return just the "id"
						let id = current_mediainfo.as_ref().map(|v| return v.id.clone());
						pgcb(DownloadProgress::SingleProgress(id, percent));
					}
				},
				LineType::Custom => {
					if let Some(parsed_type) = linetype.try_get_parse_helper(line) {
						match parsed_type {
							CustomParseType::Start(mi) => {
								debug!(
									"Found PARSE_START: \"{}\" \"{}\" \"{:?}\"",
									mi.id, mi.provider, mi.title
								);
								if current_mediainfo.is_some() {
									warn!("Found PARSE_START, but \"current_mediainfo\" is still \"Some\"");
								}
								current_mediainfo = Some(mi);
								// the following uses "unwrap", because the option has been set by the previous line
								let c_mi = current_mediainfo.as_ref().unwrap();
								// the following also uses "expect", because "try_get_parse_helper" is guranteed to return with id, title, provider for "PARSE_START"
								let title = c_mi
									.title
									.as_ref()
									.expect("current_mediainfo.title should have been set");
								pgcb(DownloadProgress::SingleStarting(c_mi.id.clone(), title.to_string()));
							},
							CustomParseType::End(mi) => {
								debug!("Found PARSE_END: \"{}\" \"{}\"", mi.id, mi.provider);

								if let Some(last_mediainfo) = current_mediainfo.take() {
									pgcb(DownloadProgress::SingleFinished(mi.id.clone())); // callback inside here, because it should only be triggered if there was a media_info to take
									if mi.id != last_mediainfo.id {
										// warn in the weird case where the "current_mediainfo" and result from PARSE_END dont match
										warn!(
											"Found PARSE_END, but the ID does dont match with \"current_mediainfo\"!"
										);
									}

									// do not add videos to "mediainfo_vec", unless the media had actually been downloaded
									if had_download {
										mediainfo_vec.push(last_mediainfo);
									}
								} else {
									// write a log that PARSE_END was present but was None (like in the case of a Error happening)
									debug!("Found a PARSE_END, but \"current_mediainfo\" was \"None\"!");
								}

								// reset the value for the next download
								had_download = false;
							},
							CustomParseType::Playlist(count) => {
								debug!("Found PLAYLIST {count}");
								pgcb(DownloadProgress::PlaylistInfo(count));
							},
							CustomParseType::Move(mi) => {
								debug!("Found MOVE: \"{}\" \"{}\" \"{:?}\"", mi.id, mi.provider, mi.filename);

								if let Some(last_mediainfo) = current_mediainfo.as_mut() {
									last_mediainfo.set_filename(
										mi.filename.expect(
											"Expected try_get_parse_helper to return a mediainfo with filename",
										),
									);
								} else {
									warn!("Found MOVE, but did not have a current_mediainfo");
								}
							},
						}
					}
				},
				LineType::Skip => {
					pgcb(DownloadProgress::Skipped(1));
				},
				LineType::Error => {
					// the following is using debug printing, because the line may include escape characters, which would mess-up the printing, but is still good to know when reading
					warn!("Encountered youtube-dl error: {:#?}", line);
					last_error = Some(crate::Error::other(line));
					pgcb(DownloadProgress::Skipped(1));
					current_mediainfo.take(); // replace with none, because this media should not be added
				},
			}
		} else if !line.is_empty() {
			info!("No type has been found for line \"{}\"", line);
		}
	}

	// report that downloading is now finished
	pgcb(DownloadProgress::AllFinished(mediainfo_vec.len()));

	if let Some(last_error) = last_error {
		return Err(last_error);
	}

	return Ok(());
}

#[cfg(test)]
mod test {
	use std::path::PathBuf;
	use std::sync::atomic::AtomicUsize;
	use std::sync::Arc;
	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	use super::*;

	/// Test Implementation for [`DownloadOptions`]
	struct TestOptions {
		audio_only:           bool,
		extra_arguments:      Vec<PathBuf>,
		download_path:        PathBuf,
		url:                  String,
		archive_lines:        Vec<String>,
		print_command_stdout: bool,
		count_estimate:       usize,
		sub_langs:            Option<String>,
		ytdl_version:         chrono::NaiveDate,
	}

	impl TestOptions {
		/// Helper Function for easily creating a new instance of [`TestOptions`] for [`assemble_ytdl_command`] testing
		pub fn new_assemble(
			audio_only: bool,
			extra_arguments: Vec<PathBuf>,
			download_path: PathBuf,
			url: String,
			archive_lines: Vec<String>,
		) -> Self {
			return Self {
				audio_only,
				extra_arguments,
				download_path,
				url,
				archive_lines,
				..Default::default()
			};
		}

		/// Helper Function for easily creating a new instance of [`TestOptions`] for [`handle_stdout`] testing
		pub fn new_handle_stdout(print_command_stdout: bool, count_estimate: usize) -> Self {
			return Self {
				print_command_stdout,
				count_estimate,
				..Default::default()
			};
		}

		/// Test with a custom ytdl_version
		pub fn with_version(mut self, ytdl_version: chrono::NaiveDate) -> Self {
			self.ytdl_version = ytdl_version;

			return self;
		}

		/// Get the test default version
		pub fn default_version() -> chrono::NaiveDate {
			// return current date plus 1 year to activate all features for now
			return chrono::offset::Utc::now()
				.naive_utc()
				.checked_add_months(chrono::Months::new(12))
				.unwrap()
				.into();
		}
	}

	impl Default for TestOptions {
		fn default() -> Self {
			return Self {
				audio_only:           false,
				extra_arguments:      Vec::default(),
				download_path:        PathBuf::default(),
				url:                  String::default(),
				archive_lines:        Vec::default(),
				print_command_stdout: false,
				count_estimate:       0,
				sub_langs:            None,
				ytdl_version:         Self::default_version(),
			};
		}
	}

	impl DownloadOptions for TestOptions {
		fn audio_only(&self) -> bool {
			return self.audio_only;
		}

		fn download_path(&self) -> &std::path::Path {
			return &self.download_path;
		}

		fn get_url(&self) -> &str {
			return &self.url;
		}

		fn gen_archive(&self, _connection: &mut SqliteConnection) -> Option<Box<dyn Iterator<Item = String> + '_>> {
			if self.archive_lines.is_empty() {
				return None;
			}

			return Some(Box::from(self.archive_lines.iter().map(|v| return v.clone())));
		}

		fn extra_ytdl_arguments(&self) -> Vec<&std::ffi::OsStr> {
			return self.extra_arguments.iter().map(|v| return v.as_os_str()).collect();
		}

		fn print_command_stdout(&self) -> bool {
			return self.print_command_stdout;
		}

		fn get_count_estimate(&self) -> usize {
			return self.count_estimate;
		}

		fn sub_langs(&self) -> Option<&String> {
			return self.sub_langs.as_ref();
		}

		fn ytdl_version(&self) -> chrono::NaiveDate {
			return self.ytdl_version;
		}
	}

	/// Test helper function to create a connection AND get a clean testing dir path
	fn create_connection() -> (SqliteConnection, TempDir, PathBuf) {
		let testdir = TempBuilder::new()
			.prefix("ytdl-test-download-")
			.tempdir()
			.expect("Expected a temp dir to be created");
		// chrono is used to create a different database for each thread
		let path = testdir.as_ref().join(format!("{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		let parent = testdir.as_ref().to_owned();

		return (
			crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start"),
			testdir,
			parent,
		);
	}

	/// Test utility function for easy callbacks
	fn callback_counter<'a>(
		index_pg: &'a Arc<AtomicUsize>,
		expected_pg: &'a Vec<DownloadProgress>,
	) -> impl FnMut(DownloadProgress) + 'a {
		return |imp| {
			let index = index_pg.load(std::sync::atomic::Ordering::Relaxed);
			// panic in case there are more events than expected, with a more useful message than default
			assert!(
				index <= expected_pg.len(),
				"index_pg is higher than provided expected_pg values! (more events than expected?)"
			);
			assert_eq!(expected_pg[index], imp);
			index_pg.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
		};
	}

	mod argshelper {
		use std::path::Path;

		use super::*;

		#[test]
		fn test_basic() {
			let mut args = ArgsHelper::new();
			args.arg("someString");
			args.arg(Path::new("somePath"));

			assert_eq!(
				args.into_inner(),
				vec![OsString::from("someString"), OsString::from("somePath")]
			);
		}

		#[test]
		fn test_into_vec() {
			let mut args = ArgsHelper::new();
			args.arg("someString");
			args.arg(Path::new("somePath"));

			assert_eq!(
				Vec::from(args),
				vec![OsString::from("someString"), OsString::from("somePath")]
			);
		}
	}

	mod assemble_ytdl_command {
		use super::*;

		fn create_dl_dir() -> (PathBuf, TempDir) {
			let testdir = TempBuilder::new()
				.prefix("ytdl-test-dlAssemble-")
				.tempdir()
				.expect("Expected a temp dir to be created");

			return (testdir.as_ref().to_owned(), testdir);
		}

		#[test]
		fn test_basic_assemble() {
			let (dl_dir, _tempdir) = create_dl_dir();
			let options = TestOptions::new_assemble(
				false,
				Vec::default(),
				dl_dir.clone(),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("--no-quiet"),
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--convert-thumbnails"),
					OsString::from("webp>jpg"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PLAYLIST '%(playlist_count)s'"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--print"),
					OsString::from("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					dl_dir.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s").into(),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_audio_only() {
			let (dl_dir, _tempdir) = create_dl_dir();
			let options = TestOptions::new_assemble(
				true,
				Vec::default(),
				dl_dir.clone(),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("--no-quiet"),
					OsString::from("-f"),
					OsString::from("bestaudio/best"),
					OsString::from("-x"),
					OsString::from("--audio-format"),
					OsString::from("mp3"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--convert-thumbnails"),
					OsString::from("webp>jpg"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PLAYLIST '%(playlist_count)s'"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--print"),
					OsString::from("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					dl_dir.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s").into(),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_extra_arguments() {
			let (dl_dir, _tempdir) = create_dl_dir();
			let options = TestOptions::new_assemble(
				false,
				vec![PathBuf::from("hello1")],
				dl_dir.clone(),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("--no-quiet"),
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--convert-thumbnails"),
					OsString::from("webp>jpg"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PLAYLIST '%(playlist_count)s'"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--print"),
					OsString::from("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					dl_dir.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s").into(),
					OsString::from("hello1"),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_archive() {
			let (mut connection, _tempdir, test_dir) = create_connection();
			let options = TestOptions::new_assemble(
				false,
				Vec::default(),
				test_dir.clone(),
				"someURL".to_owned(),
				vec!["line 1".to_owned(), "line 2".to_owned()],
			);

			let ret = assemble_ytdl_command(Some(&mut connection), &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			let pid = std::process::id();

			assert_eq!(
				ret,
				vec![
					OsString::from("--download-archive"),
					test_dir.join(format!("ytdl_archive_{pid}.txt")).as_os_str().to_owned(),
					OsString::from("--no-quiet"),
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--convert-thumbnails"),
					OsString::from("webp>jpg"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PLAYLIST '%(playlist_count)s'"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--print"),
					OsString::from("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					test_dir
						.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s")
						.as_os_str()
						.to_owned(),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_all_options_together() {
			let (mut connection, _tempdir, test_dir) = create_connection();
			let options = {
				let mut o = TestOptions::new_assemble(
					true,
					vec![PathBuf::from("hello1")],
					test_dir.clone(),
					"someURL".to_owned(),
					vec!["line 1".to_owned(), "line 2".to_owned()],
				);
				o.sub_langs = Some("en-US".to_owned());

				o
			};

			let ret = assemble_ytdl_command(Some(&mut connection), &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			let pid = std::process::id();

			assert_eq!(
				ret,
				vec![
					OsString::from("--download-archive"),
					test_dir.join(format!("ytdl_archive_{pid}.txt")).as_os_str().to_owned(),
					OsString::from("--no-quiet"),
					OsString::from("-f"),
					OsString::from("bestaudio/best"),
					OsString::from("-x"),
					OsString::from("--audio-format"),
					OsString::from("mp3"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--convert-thumbnails"),
					OsString::from("webp>jpg"),
					OsString::from("--write-thumbnail"),
					OsString::from("--embed-subs"),
					OsString::from("--write-subs"),
					OsString::from("--sub-langs"),
					OsString::from("en-US"),
					OsString::from("--ppa"),
					OsString::from("EmbedSubtitle:-disposition:s:0 default"),
					OsString::from("--print"),
					OsString::from("before_dl:PLAYLIST '%(playlist_count)s'"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--print"),
					OsString::from("after_move:MOVE '%(extractor)s' '%(id)s' %(filepath)s"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					test_dir
						.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s")
						.as_os_str()
						.to_owned(),
					OsString::from("hello1"),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_quiet_version_gate() {
			let (dl_dir, _tempdir) = create_dl_dir();

			// test version before
			{
				#[allow(clippy::zero_prefixed_literal)]
				let options = TestOptions::new_assemble(
					true,
					Vec::default(),
					dl_dir.clone(),
					"someURL".to_owned(),
					Vec::default(),
				)
				.with_version(chrono::NaiveDate::from_ymd_opt(2023, 3, 04).unwrap());

				let ret = assemble_ytdl_command(None, &options);

				assert!(ret.is_ok());
				let ret = ret.expect("Expected is_ok check to pass");

				assert!(!ret.contains(&OsString::from("--no-quiet")));
			}

			// test version after
			{
				let options = TestOptions::new_assemble(
					true,
					Vec::default(),
					dl_dir.clone(),
					"someURL".to_owned(),
					Vec::default(),
				)
				.with_version(chrono::NaiveDate::from_ymd_opt(2023, 3, 25).unwrap());

				let ret = assemble_ytdl_command(None, &options);

				assert!(ret.is_ok());
				let ret = ret.expect("Expected is_ok check to pass");

				assert!(ret.contains(&OsString::from("--no-quiet")));
			}
		}
	}

	mod linetype_impls {
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

	mod handle_stdout {
		use super::*;

		#[test]
		fn test_basic_single_usage() {
			let expected_pg = &vec![
				DownloadProgress::AllStarting,
				DownloadProgress::SingleStarting("-----------".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleFinished("-----------".to_owned()),
				DownloadProgress::AllFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false, 1);

			let input = r#"
PARSE_START 'youtube' '-----------' Some Title Here
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '-----------'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("-----------", "youtube").with_title("Some Title Here")],
				media_vec
			);
		}

		#[test]
		fn test_basic_multi_usage() {
			let expected_pg = &vec![
				DownloadProgress::AllStarting,
				DownloadProgress::SingleStarting("----------0".to_owned(), "Some Title Here 0".to_owned()),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------0".to_owned()), 100),
				DownloadProgress::SingleFinished("----------0".to_owned()),
				DownloadProgress::SingleStarting("----------1".to_owned(), "Some Title Here 1".to_owned()),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("----------1".to_owned()), 100),
				DownloadProgress::SingleFinished("----------1".to_owned()),
				DownloadProgress::AllFinished(2),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false, 1);

			let input = r#"
PARSE_START 'youtube' '----------0' Some Title Here 0
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '----------0'
PARSE_START 'soundcloud' '----------1' Some Title Here 1
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'soundcloud' '----------1'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(2, media_vec.len());

			assert_eq!(
				vec![
					MediaInfo::new("----------0", "youtube").with_title("Some Title Here 0"),
					MediaInfo::new("----------1", "soundcloud").with_title("Some Title Here 1")
				],
				media_vec
			);
		}

		#[test]
		fn test_skipped() {
			let expected_pg = &vec![
				DownloadProgress::AllStarting,
				DownloadProgress::SingleStarting("-----------".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 50),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 57),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("-----------".to_owned()), 100),
				DownloadProgress::SingleFinished("-----------".to_owned()),
				DownloadProgress::Skipped(1),
				DownloadProgress::AllFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false, 1);

			let input = r#"
PARSE_START 'youtube' '-----------' Some Title Here
[download]   0.0% of 78.44MiB at 207.76KiB/s ETA 06:27
[download]  50.0% of 78.44MiB at 526.19KiB/s ETA 01:16
[download] 100% of 78.44MiB at  5.89MiB/s ETA 00:00
[download] 100% of 78.44MiB in 00:07
[download]   0.0% of 3.47MiB at 196.76KiB/s ETA 00:18
[download]  57.6% of 3.47MiB at  9.57MiB/s ETA 00:00
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
PARSE_END 'youtube' '-----------'
[youtube] someId: has already been recorded in the archive
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("-----------", "youtube").with_title("Some Title Here")],
				media_vec
			);
		}

		/// Test to test skipping, erroring and normal download together
		#[test]
		fn test_skip_error_and_normal() {
			let expected_pg = &vec![
				DownloadProgress::AllStarting,
				DownloadProgress::Skipped(1), // one archive skip
				DownloadProgress::Skipped(1), // one archive skip
				DownloadProgress::Skipped(1), // one error skip
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 100),
				DownloadProgress::SingleFinished("someid4".to_owned()),
				DownloadProgress::AllFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false, 1);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4
[download] Downloading item 1 of 4
[aprovider] someid1: has already been recorded in the archive
[download] Downloading item 2 of 4
[aprovider] someid2: has already been recorded in the archive
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://someurl.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
ERROR: [aprovider] someid3: somekinda error
[download] Downloading item 4 of 4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Some Title Here [someid4].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid4' /path/to/somewhere
PARSE_END 'aprovider' 'someid4'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("someid4", "aprovider")
					.with_title("Some Title Here")
					.with_filename("somewhere")],
				media_vec
			);
		}

		/// Test that when a error happens while downloading that the media is not added as a final media
		#[test]
		fn test_error_while_downloading() {
			let expected_pg = &vec![
				DownloadProgress::AllStarting,
				DownloadProgress::SingleStarting("someid1".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 0),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 100),
				DownloadProgress::SingleProgress(Some("someid1".to_owned()), 100),
				DownloadProgress::SingleFinished("someid1".to_owned()),
				DownloadProgress::SingleStarting("someid2".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid2".to_owned()), 2),
				DownloadProgress::Skipped(1), // one error skip
				DownloadProgress::SingleStarting("someid3".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid3".to_owned()), 0),
				DownloadProgress::Skipped(1), // one error skip
				DownloadProgress::SingleStarting("someid4".to_owned(), "Some Title Here".to_owned()),
				DownloadProgress::SingleProgress(Some("someid4".to_owned()), 0),
				DownloadProgress::Skipped(1), // one error skip
				DownloadProgress::AllFinished(1),
			];
			let expect_index = Arc::new(AtomicUsize::new(0));

			let options = TestOptions::new_handle_stdout(false, 1);

			let input = r#"
[aprovider] Extracting URL: https://someurl.com/hello
[download] Downloading playlist: someplaylist
[aprovider] someplaylist: Downloading page 0
[aprovider] Playlist someplaylist: Downloading 4 items of 4

[download] Downloading item 1 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid2
[aprovider] someid1: Downloading JSON metadata
[info] someid1: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid1' Some Title Here
[download] Destination: Some Title Here [someid1].mp4
[download]   0.1% of  3.47MiB at  10.57MiB/s ETA 09:37
[download] 100% of 3.47MiB at 10.57MiB/s ETA 00:00
[download] 100% of 3.47MiB in 00:00
MOVE 'aprovider' 'someid1' /path/to/somewhere
PARSE_END 'aprovider' 'someid1'

[download] Downloading item 2 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid2
[aprovider] someid2: Downloading JSON metadata
[info] someid2: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid2' Some Title Here
[download] Destination: Happy Halloween Mona [someid2].mp4
[download]   2.7% of  5.00MiB at    4.18MiB/s ETA 01:09

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid2'
[download] Downloading item 3 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid3
[aprovider] someid3: Downloading JSON metadata
[info] someid3: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid3' Some Title Here
[download] Destination: Pjanoo Mona [someid3].mp4
[download]   0.0% of  6.00MiB at  Unknown B/s ETA Unknown

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid3'
[download] Downloading item 4 of 4
[aprovider] Extracting URL: https://aprovider.com/video/someid4
[aprovider] someid4: Downloading JSON metadata
[info] someid4: Downloading 1 format(s): Source
PARSE_START 'aprovider' 'someid4' Some Title Here
[download] Destination: Girls Mona [someid4].mp4
[download]   0.0% of  7.00MiB at  Unknown B/s ETA Unknown

ERROR: unable to write data: [Errno 28] No space left on device

PARSE_END 'aprovider' 'someid4'
			"#;

			let mut media_vec: Vec<MediaInfo> = Vec::new();

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
				&mut media_vec,
			);

			assert!(res.is_ok());

			assert_eq!(1, media_vec.len());

			assert_eq!(
				vec![MediaInfo::new("someid1", "aprovider")
					.with_title("Some Title Here")
					.with_filename("somewhere")],
				media_vec
			);
		}
	}
}
