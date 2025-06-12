use std::{
	ffi::OsString,
	fs::File,
	io::{
		BufWriter,
		Write as _,
	},
	path::Path,
};

use diesel::SqliteConnection;

use crate::{
	error::IOErrorToError as _,
	main::download::get_archive_name,
};

use super::download_options::DownloadOptions;

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

/// Helper Function to assemble all ytdl command arguments
/// Returns a list of arguments for youtube-dl in order
#[inline]
pub fn assemble_ytdl_command<A: DownloadOptions>(
	connection: Option<&mut SqliteConnection>,
	options: &A,
) -> Result<Vec<OsString>, crate::Error> {
	let mut ytdl_args = ArgsHelper::new();

	let output_dir = options.download_path();
	debug!("YTDL Output dir is \"{}\"", output_dir.to_string_lossy());

	std::fs::create_dir_all(output_dir).attach_path_err(output_dir)?;

	// set a custom format the videos will be in for consistent parsing
	let output_format = output_dir.join("'%(extractor)s'-'%(id)s'-%(title).150B.%(ext)s");

	generate_archive(&mut ytdl_args, connection, options, output_dir)?;

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
		ytdl_args.arg("--audio-format").arg(options.get_audio_format());
	} else {
		// set the format that should be downloaded
		ytdl_args.arg("-f").arg("bestvideo+bestaudio/best");
		// set final consistent output format
		ytdl_args.arg("--remux-video").arg(options.get_video_format());
	}

	// embed the videoo thumbnail if available into the output container
	ytdl_args.arg("--embed-thumbnail");

	// add metadata to the container if the container supports it
	ytdl_args.arg("--add-metadata");

	// the following is mainly because of https://github.com/yt-dlp/yt-dlp/issues/4227
	ytdl_args.arg("--convert-thumbnails").arg("webp>jpg"); // convert webp thumbnails to jpg

	// write the media's thumbnail as a seperate file
	ytdl_args.arg("--write-thumbnail");

	add_subs(&mut ytdl_args, options);

	add_prints(&mut ytdl_args);

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

/// Generate the ytdl archive, if necessary
fn generate_archive<A: DownloadOptions>(
	ytdl_args: &mut ArgsHelper,
	connection: Option<&mut SqliteConnection>,
	options: &A,
	output_dir: &Path,
) -> Result<(), crate::Error> {
	// no connection, nothing to generate
	let Some(connection) = connection else {
		return Ok(());
	};

	debug!("Found connection, generating archive");

	// we have a connection, but the implementor didnt want a ytdl archive file or arguments
	// Note: if this returns none, this means there will be no ytdlr archive file or argument,
	// which also means that ytdl will not output a ytdl archive
	let Some(archive_lines) = options.gen_archive(connection) else {
		debug!("Found connection, but didnt generate any lines.");
		return Ok(());
	};

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

	return Ok(());
}

/// Add subtitle arguments, if necessary
fn add_subs<A: DownloadOptions>(ytdl_args: &mut ArgsHelper, options: &A) {
	let Some(sub_langs) = options.sub_langs() else {
		return;
	};

	// add subtitles directly into the downloaded file - if available
	ytdl_args.arg("--embed-subs");

	// write subtiles as a separate file
	ytdl_args.arg("--write-subs");

	// set which subtitles to download
	ytdl_args.arg("--sub-langs").arg(sub_langs);

	// set subtitle stream as default directly in the ytdl post-processing
	ytdl_args.arg("--ppa").arg("EmbedSubtitle:-disposition:s:0 default"); // set stream 0 as default
}

/// Add the custom print statements used for detecting different stages and information
fn add_prints(ytdl_args: &mut ArgsHelper) {
	// set custom ytdl logging for easy parsing

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

#[cfg(test)]
mod test {
	use std::path::PathBuf;

	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	use crate::main::download::test_utils::{
		TestOptions,
		create_connection,
	};

	use super::*;

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
	fn test_audio_custom_format() {
		let (dl_dir, _tempdir) = create_dl_dir();
		let options = TestOptions::new_assemble(
			true,
			Vec::default(),
			dl_dir.clone(),
			"someURL".to_owned(),
			Vec::default(),
		)
		.set_format("m4a>mp3", "webm>mp4");

		let ret = assemble_ytdl_command(None, &options);

		assert!(ret.is_ok());
		let ret = ret.expect("Expected is_ok check to pass");

		let ret: Vec<OsString> = ret
			.into_iter()
			.skip_while(|v| return v != "--audio-format")
			.take(2)
			.collect();

		assert_eq!(ret, vec![OsString::from("--audio-format"), OsString::from("m4a>mp3")]);
	}

	#[test]
	fn test_video_custom_format() {
		let (dl_dir, _tempdir) = create_dl_dir();
		let options = TestOptions::new_assemble(
			false,
			Vec::default(),
			dl_dir.clone(),
			"someURL".to_owned(),
			Vec::default(),
		)
		.set_format("m4a>mp3", "webm>mp4");

		let ret = assemble_ytdl_command(None, &options);

		assert!(ret.is_ok());
		let ret = ret.expect("Expected is_ok check to pass");

		let ret: Vec<OsString> = ret
			.into_iter()
			.skip_while(|v| return v != "--remux-video")
			.take(2)
			.collect();

		assert_eq!(ret, vec![OsString::from("--remux-video"), OsString::from("webm>mp4")]);
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
