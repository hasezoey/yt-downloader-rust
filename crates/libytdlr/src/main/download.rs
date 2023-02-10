//! Module for handling youtube-dl

use diesel::SqliteConnection;
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
	data::cache::{
		media_info::MediaInfo,
		media_provider::MediaProvider,
	},
	traits::context::DownloadOptions,
};

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadProgress {
	/// Variant representing that the download is starting
	AllStarting,
	/// Variant representing that a media has started the process (id, title)
	SingleStarting(String, String),
	/// Variant representing that a started media has increased in progress (id, progress)
	/// "id" may be [`None`] when the previous parsing did not parse a title
	SingleProgress(Option<String>, u8),
	/// Variant representing that a media has finished the process (id)
	/// the "id" is not guranteed to be the same as in [`DownloadProgress::SingleStarting`]
	SingleFinished(String),
	/// Variant representing that the download has finished (downloaded media count)
	/// The value in this tuple is the size of actually downloaded media, not just found media
	AllFinished(usize),
}

/// Download a single URL
/// Assumes ytdl and ffmpeg have already been checked to exist and work (like using [`crate::spawn::ytdl::ytdl_version`])
/// Returned [`Vec<MediaInfo>`] will not be added to the archive in this function, it has to be done afterwards
pub fn download_single<A: DownloadOptions, C: FnMut(DownloadProgress)>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
	pgcb: C,
) -> Result<Vec<MediaInfo>, crate::Error> {
	let ytdl_child = {
		let args = assemble_ytdl_command(connection, options)?;

		// merge stderr into stdout
		duct::cmd("youtube-dl", args).stderr_to_stdout().reader()?
	};

	let stdout_reader = BufReader::new(&ytdl_child);
	// let stdout_reader = BufReader::new(
	// 	ytdl_child
	// 		.stdout
	// 		.take()
	// 		.ok_or_else(|| return crate::Error::Other("Failed to take YTDL Child's STDOUT".to_owned()))?,
	// );
	// let stderr_reader = BufReader::new(
	// 	ytdl_child
	// 		.stderr
	// 		.take()
	// 		.ok_or_else(|| return crate::Error::Other("Failed to take YTDL Child's STDERR".to_owned()))?,
	// );

	// let ytdl_child_stderr_thread = std::thread::Builder::new()
	// 	.name("ytdl stderr handler".to_owned())
	// 	.spawn(move || {
	// 		// always print STDERR as "warn"
	// 		stderr_reader
	// 			.lines()
	// 			.filter_map(|line| return line.ok())
	// 			.for_each(|line| {
	// 				// this is not higher than "info" because ytdl otherwise might log some more generic messages
	// 				info!("ytdl [STDERR]: \"{}\"", line);
	// 			})
	// 	})?;

	let media_vec = handle_stdout(options, pgcb, stdout_reader)?;

	// wait until the ytdl_child has exited and get the status of the exit
	// let ytdl_child_exit_status = ytdl_child.wait()?;
	loop {
		// wait loop, because somehow a "ReaderHandle" does not implement "wait", only "try_wait", but have to wait for it to exit here
		if ytdl_child.try_wait()?.is_some() {
			break;
		}
		std::thread::sleep(Duration::from_millis(100)); // sleep to same some time between the next wait (to not cause constant cpu spike)
	}

	// wait until the stderr thread has exited
	// ytdl_child_stderr_thread.join().map_err(|err| {
	// 	return crate::Error::Other(format!("Joining the ytdl_child STDERR handle failed: {:?}", err));
	// })?;

	// if !ytdl_child_exit_status.success() {
	// 	return Err(match ytdl_child_exit_status.code() {
	// 		Some(code) => crate::Error::Other(format!("YTDL Child exited with code: {}", code)),
	// 		None => {
	// 			let signal = match ytdl_child_exit_status.signal() {
	// 				Some(code) => code.to_string(),
	// 				None => "None".to_owned(),
	// 			};

	// 			crate::Error::Other(format!("YTDL Child exited with signal: {}", signal))
	// 		},
	// 	});
	// }

	return Ok(media_vec);
}

/// Internal Struct for easily adding various types that resolve to [`OsString`] and output a [`Vec<OsString>`]
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

/// Consistent way of getting the archive name
pub fn get_archive_name(output_dir: &std::path::Path) -> std::path::PathBuf {
	return output_dir.join(format!("ytdl_archive_{}.txt", std::process::id()));
}

/// Helper Function to assemble all ytdl command arguments
/// Returns a list of arguments for youtube-dl in order
#[inline]
fn assemble_ytdl_command<A: DownloadOptions>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
) -> std::io::Result<Vec<OsString>> {
	let mut ytdl_args = ArgsHelper::new();

	let output_dir = options.download_path();
	debug!("YTDL Output dir is \"{}\"", output_dir.to_string_lossy());

	std::fs::create_dir_all(output_dir)?;

	// set a custom format the videos will be in for consistent parsing
	let output_format = output_dir.join("'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s");

	if let Some(connection) = connection {
		debug!("Found connection, generating archive");
		if let Some(archive_lines) = options.gen_archive(connection) {
			let archive_file_path = get_archive_name(output_dir);

			// write all lines to the file and drop the handle before giving the argument
			{
				let mut archive_write_handle = BufWriter::new(File::create(&archive_file_path)?);

				for archive_line in archive_lines {
					archive_write_handle.write_all(archive_line.as_bytes())?;
				}
			}

			ytdl_args.arg("--download-archive").arg(&archive_file_path);
		}
	}

	// apply options to make output audio-only
	if options.audio_only() {
		// set the output format
		ytdl_args.arg("-f").arg("bestaudio/best");
		// set ytdl to always extract the audio, if it is not already audio-only
		ytdl_args.arg("-x");
		// set the output audio format
		ytdl_args.arg("--audio-format").arg("mp3");
	} else {
		ytdl_args.arg("-f").arg("bestvideo+bestaudio/best");
		// set final consistent output format
		ytdl_args.arg("--remux-video").arg("mkv");
	}

	{
		// the following options are test-wise applied to both audio and video files

		// embed the videoo thumbnail if available into the output container
		ytdl_args.arg("--embed-thumbnail");

		// add metadata to the container if the container supports it
		ytdl_args.arg("--add-metadata");
	}

	// write the media's thumbnail as a seperate file
	ytdl_args.arg("--write-thumbnail");

	// set custom logging for easy parsing
	// print once before the video starts to download to get all information and to get a consistent start point
	ytdl_args
		.arg("--print")
		.arg("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s");
	// print once after the video got fully processed to get a consistent end point
	ytdl_args
		.arg("--print")
		// only "extractor" and "id" is required, because it can be safely assumed that when this is printed, the "PARSE_START" was also printed
		.arg("after_video:PARSE_END '%(extractor)s' '%(id)s'");

	// ensure ytdl is printing progress reports
	ytdl_args.arg("--progress");
	// ensure ytdl prints the progress reports on a new line
	ytdl_args.arg("--newline");

	// ensure it is not in simulate mode (for example set via extra arguments)
	ytdl_args.arg("--no-simulate");

	// set the output directory for ytdl
	ytdl_args.arg("-o").arg(output_format);

	// apply all extra arguments
	for extra_arg in options.extra_ytdl_arguments().iter() {
		ytdl_args.arg(extra_arg);
	}

	// apply the url to download as the last argument
	ytdl_args.arg(options.get_url());

	return Ok(ytdl_args.into());
}

/// Helper Enum for differentiating [`LineType::Custom`] from "START" and "END"
#[derive(Debug, PartialEq, Clone)]
enum CustomParseType {
	Start,
	End,
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
}

impl LineType {
	/// Try to get the correct Variant for a input line
	/// Will return [`None`] if no type has been found
	pub fn try_from_line<I: AsRef<str>>(input: I) -> Option<Self> {
		lazy_static! {
			// basic regex to test if the line is "[something] something", and if it is, return what is inside "[]"
			static ref BASIC_TYPE_REGEX: Regex = Regex::new(r"(?mi)^\[([\da-z:_]*)\]").unwrap();
			// regex to check for generic lines
			static ref GENERIC_TYPE_REGEX: Regex = Regex::new(r"(?mi)^deleting original file").unwrap();
			// regex to check for ERRORs
			static ref ERROR_TYPE_REGEX: Regex = Regex::new(r"(?m)^ERROR:").unwrap();
			// regex to check for ERRORs
			static ref YTDL_ERROR_TYPE_REGEX: Regex = Regex::new(r"(?m)^youtube-dl: error:").unwrap();
		}

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

			// everything that is not specially handled before, will get treated as being a provider
			return Some(Self::ProviderSpecific);
		}

		// check for Generic lines that dont have a prefix
		if GENERIC_TYPE_REGEX.is_match(input) {
			return Some(Self::Generic);
		}

		// matches both "PARSE_START" and "PARSE_END"
		if input.starts_with("PARSE") {
			return Some(Self::Custom);
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

		lazy_static! {
			// regex to get the percentage from the input line
			static ref DOWNLOAD_PERCENTAGE_REGEX: Regex = Regex::new(r"(?mi)^^\[[\da-z:_]*\]\s+(\d{1,3})(?:\.\d)?%").unwrap();
		}

		let input = input.as_ref();

		if let Some(cap) = DOWNLOAD_PERCENTAGE_REGEX.captures(input) {
			let percent_str = &cap[1];

			// directly use the "Result"returned by "from_str_radix" and convert it to a "Option"
			return percent_str.parse::<u8>().ok();
		}

		return None;
	}

	/// Try to get the Custom Set Parse helper from input
	/// Retrun [`None`] if not being of variant [`LineType::Custom`] or if not parse helper can be found
	/// Tuple fields: (mediaprovider, id, title)
	pub fn try_get_parse_helper<I: AsRef<str>>(&self, input: I) -> Option<(CustomParseType, MediaInfo)> {
		// this function only works with Custom lines
		if self != &Self::Custom {
			return None;
		}

		lazy_static! {
			// regex to get all information from the Parsing helper
			static ref PARSER_HELPER_REGEX: Regex = Regex::new(r"(?mi)^PARSE_(START|END) '([^']+)' '([^']+)'(?: (.+))?$").unwrap();
		}

		let input = input.as_ref();

		if let Some(cap) = PARSER_HELPER_REGEX.captures(input) {
			let line_type = &cap[1];
			let provider = &cap[2];
			let id = &cap[3];

			match line_type {
				"START" => {
					let title = &cap[4];

					return Some((
						CustomParseType::Start,
						MediaInfo::new(id)
							.with_title(title)
							.with_provider(MediaProvider::from_str_like(provider)),
					));
				},
				"END" => {
					return Some((
						CustomParseType::End,
						MediaInfo::new(id).with_provider(MediaProvider::from_str_like(provider)),
					));
				},
				// the following is unreachable, because the Regex ensures that only "START" and "END" match
				_ => unreachable!(),
			}
		}

		return None;
	}
}

/// Helper function to handle the output from a spawned ytdl command
/// Returns all processed (not skipped) Medias as [`Vec<MediaInfo>`]
#[inline]
fn handle_stdout<A: DownloadOptions, C: FnMut(DownloadProgress), R: BufRead>(
	// connection: Option<&mut SqliteConnection>,
	options: &A,
	mut pgcb: C,
	reader: R,
) -> Result<Vec<MediaInfo>, crate::Error> {
	// report that the downloading is now starting
	pgcb(DownloadProgress::AllStarting);

	// cache the bool for "print_command_stdout" to not execute the function for every line (should be a static value)
	let print_stdout = options.print_command_stdout();

	// the array where finished "current_mediainfo" gets appended to
	// for performance / allocation efficiency, a count is requested from options
	let mut mediainfo_vec: Vec<MediaInfo> = Vec::with_capacity(options.get_count_estimate());
	// "current_mediainfo" may not be defined because it cannot be guranteed that a parsed output was emitted
	let mut current_mediainfo: Option<MediaInfo> = None;
	// value to determine if a media has actually been downloaded, or just found
	let mut had_download = false;

	for line in reader.lines().filter_map(|line| return line.ok()) {
		// only print STDOUT to output when requested
		if print_stdout {
			trace!("ytdl [STDOUT]: \"{}\"", line);
		}

		if let Some(linetype) = LineType::try_from_line(&line) {
			match linetype {
				// currently there is nothing that needs to be done with "Ffmpeg" lines
				LineType::Ffmpeg => (),
				LineType::Download => {
					had_download = true;
					if let Some(percent) = linetype.try_get_download_percent(line) {
						// convert "current_mediainfo" to a reference and operate on the inner value (if exists) to return just the "id"
						let id = current_mediainfo.as_ref().map(|v| return v.id.clone());
						pgcb(DownloadProgress::SingleProgress(id, percent));
					}
				},
				// currently there is nothing that needs to be done with "ProviderSpecific" Lines, thanks to "--print"
				LineType::ProviderSpecific => (),
				// currently there is nothing that needs to be done with "Generic" Lines
				LineType::Generic => (),
				LineType::Custom => {
					if let Some(mi) = linetype.try_get_parse_helper(line) {
						match mi.0 {
							CustomParseType::Start => {
								debug!(
									"Found PARSE_START: \"{}\" \"{:?}\" \"{:?}\"",
									mi.1.id, mi.1.provider, mi.1.title
								);
								if current_mediainfo.is_some() {
									warn!("Found PARSE_START, but \"current_mediainfo\" is still \"Some\"");
								}
								current_mediainfo = Some(mi.1);
								// the following uses "expect", because the option has been set by the previous line
								let c_mi = current_mediainfo
									.as_ref()
									.expect("current_mediainfo should have been set");
								// the following also uses "expect", because "try_get_parse_helper" is guranteed to return with id, title, provider
								let title = c_mi
									.title
									.as_ref()
									.expect("current_mediainfo.title should have been set");
								pgcb(DownloadProgress::SingleStarting(c_mi.id.clone(), title.to_string()))
							},
							CustomParseType::End => {
								debug!("Found PARSE_END: \"{}\" \"{:?}\"", mi.1.id, mi.1.provider);
								pgcb(DownloadProgress::SingleFinished(mi.1.id.clone()));

								if let Some(last_mediainfo) = current_mediainfo.take() {
									if mi.1.id != last_mediainfo.id {
										// warn in the weird case where the "current_mediainfo" and result from PARSE_END dont match
										warn!("Found PARSE_END, but the ID's dont match with \"current_mediainfo\"!");
									}

									// do not add videos to "mediainfo_vec", unless the media had actually been downloaded
									if had_download {
										mediainfo_vec.push(last_mediainfo);
									}
								} else {
									// warn in the weird case of "current_mediainfo" being "None"
									warn!("Found a PARSE_END, but \"current_mediainfo\" was \"None\"!");
								}

								// reset the value for the next download
								had_download = false;
							},
						}
					}
				},
				LineType::Error => {
					return Err(crate::Error::Other(line));
				},
			}
		} else {
			info!("No type has been found for line \"{}\"", line);
		}
	}

	// report that downloading is now finished
	pgcb(DownloadProgress::AllFinished(mediainfo_vec.len()));

	return Ok(mediainfo_vec);
}

#[cfg(test)]
mod test {
	use std::path::PathBuf;
	use std::sync::atomic::AtomicUsize;
	use std::sync::Arc;

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
				print_command_stdout: false,
				count_estimate: 0,
			};
		}

		/// Helper Function for easily creating a new instance of [`TestOptions`] for [`handle_stdout`] testing
		pub fn new_handle_stdout(print_command_stdout: bool, count_estimate: usize) -> Self {
			return Self {
				audio_only: false,
				extra_arguments: Vec::default(),
				download_path: PathBuf::default(),
				url: String::default(),
				archive_lines: Vec::default(),
				print_command_stdout,
				count_estimate,
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

		fn get_url<'a>(&'a self) -> &'a str {
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
	}

	/// Test helper function to create a connection AND get a clean testing dir path
	fn create_connection() -> (SqliteConnection, PathBuf) {
		// chrono is used to create a different database for each thread
		let path = std::env::temp_dir().join(format!("ytdl-test-download/{}-sqlite.db", chrono::Utc::now()));

		// remove if already exists to have a clean test
		if path.exists() {
			std::fs::remove_file(&path).expect("Expected the file to be removed");
		}

		let path_parent = path.parent().expect("Expected the file to have a parent");
		std::fs::create_dir_all(path_parent).expect("expected the directory to be created");

		return (
			crate::main::sql_utils::sqlite_connect(&path).expect("Expected SQLite to successfully start"),
			path_parent.into(),
		);
	}

	/// Test utility function for easy callbacks
	fn callback_counter<'a>(
		index_pg: &'a Arc<AtomicUsize>,
		expected_pg: &'a Vec<DownloadProgress>,
	) -> impl FnMut(DownloadProgress) + 'a {
		return |imp| {
			let index = index_pg.load(std::sync::atomic::Ordering::Relaxed);
			if index > expected_pg.len() {
				// panic in case there are more events than expected, with a more useful message than default
				panic!("index_pg is higher than provided expected_pg values! (more events than expected?)");
			}
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
		use serial_test::serial;

		use super::*;

		#[test]
		fn test_basic_assemble() {
			let options = TestOptions::new_assemble(
				false,
				Vec::default(),
				PathBuf::from("/tmp/hello"),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					OsString::from("/tmp/hello/'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s"),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_audio_only() {
			let options = TestOptions::new_assemble(
				true,
				Vec::default(),
				PathBuf::from("/tmp/hello"),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("-f"),
					OsString::from("bestaudio/best"),
					OsString::from("-x"),
					OsString::from("--audio-format"),
					OsString::from("mp3"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					OsString::from("/tmp/hello/'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s"),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		fn test_extra_arguments() {
			let options = TestOptions::new_assemble(
				false,
				vec![PathBuf::from("hello1")],
				PathBuf::from("/tmp/hello"),
				"someURL".to_owned(),
				Vec::default(),
			);

			let ret = assemble_ytdl_command(None, &options);

			assert!(ret.is_ok());
			let ret = ret.expect("Expected is_ok check to pass");

			assert_eq!(
				ret,
				vec![
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					OsString::from("/tmp/hello/'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s"),
					OsString::from("hello1"),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		#[serial]
		fn test_archive() {
			let (mut connection, test_dir) = create_connection();
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
					OsString::from("-f"),
					OsString::from("bestvideo+bestaudio/best"),
					OsString::from("--remux-video"),
					OsString::from("mkv"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					test_dir
						.join("'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s")
						.as_os_str()
						.to_owned(),
					OsString::from("someURL"),
				]
			);
		}

		#[test]
		#[serial]
		fn test_all_options_together() {
			let (mut connection, test_dir) = create_connection();
			let options = TestOptions::new_assemble(
				true,
				vec![PathBuf::from("hello1")],
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
					OsString::from("-f"),
					OsString::from("bestaudio/best"),
					OsString::from("-x"),
					OsString::from("--audio-format"),
					OsString::from("mp3"),
					OsString::from("--embed-thumbnail"),
					OsString::from("--add-metadata"),
					OsString::from("--write-thumbnail"),
					OsString::from("--print"),
					OsString::from("before_dl:PARSE_START '%(extractor)s' '%(id)s' %(title)s"),
					OsString::from("--print"),
					OsString::from("after_video:PARSE_END '%(extractor)s' '%(id)s'"),
					OsString::from("--progress"),
					OsString::from("--newline"),
					OsString::from("--no-simulate"),
					OsString::from("-o"),
					test_dir
						.join("'%(extractor)s'-'%(id)s'-%(title)s.%(ext)s")
						.as_os_str()
						.to_owned(),
					OsString::from("hello1"),
					OsString::from("someURL"),
				]
			);
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
				Some((
					CustomParseType::Start,
					MediaInfo::new("-----------")
						.with_provider(MediaProvider::from_str_like("youtube"))
						.with_title("Some Title Here")
				)),
				LineType::Custom.try_get_parse_helper(input)
			);

			// should find "PARSE_END" and get "provider, id"
			let input = "PARSE_END 'youtube' '-----------'";
			assert_eq!(
				Some((
					CustomParseType::End,
					MediaInfo::new("-----------").with_provider(MediaProvider::from_str_like("youtube"))
				)),
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

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
			);

			assert!(res.is_ok());
			let res = res.expect("Expected assert to fail before this");

			assert_eq!(1, res.len());

			assert_eq!(
				vec![MediaInfo::new("-----------")
					.with_provider(MediaProvider::Youtube)
					.with_title("Some Title Here")],
				res
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

			let res = handle_stdout(
				&options,
				callback_counter(&expect_index, expected_pg),
				BufReader::new(input.as_bytes()),
			);

			assert!(res.is_ok());
			let res = res.expect("Expected assert to fail before this");

			assert_eq!(2, res.len());

			assert_eq!(
				vec![
					MediaInfo::new("----------0")
						.with_provider(MediaProvider::Youtube)
						.with_title("Some Title Here 0"),
					MediaInfo::new("----------1")
						.with_provider(MediaProvider::Soundcloud)
						.with_title("Some Title Here 1")
				],
				res
			);
		}
	}
}
