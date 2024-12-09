//! Module for Re-Applying Thumbnails to media

use std::{
	ffi::{
		OsStr,
		OsString,
	},
	fs::File,
	io::{
		BufRead,
		BufReader,
	},
	os::unix::prelude::ExitStatusExt,
	path::{
		Path,
		PathBuf,
	},
	process::Stdio,
};

use lofty::{
	config::WriteOptions,
	file::TaggedFileExt,
	picture::{
		Picture,
		PictureType,
	},
	probe::Probe,
	tag::TagExt,
};

use crate::error::{
	CustomThreadJoin,
	IOErrorToError,
};

/// Re-Apply a thumbnail from `image` onto `media` as `output`
/// Where the output is added with a "tmp" to the `output` until finished
/// Will convert input images to jpg
pub fn re_thumbnail_with_tmp<M: AsRef<Path>, I: AsRef<Path>, O: AsRef<Path>>(
	media: M,
	image: I,
	output: O,
) -> Result<(), crate::Error> {
	let mut output_path_tmp = output.as_ref().to_owned();

	// Generate a temporary filename, while leaving everything else like it was before
	{
		let mut stem = output_path_tmp
			.file_stem()
			.expect("Expected Output to be a file with name")
			.to_os_string();

		stem.push("_"); // add "_" to seperate the original name with the temporary one
		stem.push(std::process::id().to_string()); // add the current pid, so multiple instances can run at the same time

		stem.push(
			output_path_tmp
				.extension()
				// map extension to a extension with "."
				.map_or_else(
					|| return OsString::from(""),
					|v| {
						let mut tmp = OsString::from(".");

						tmp.push(v);

						return tmp;
					},
				),
		); // push original extension, because there is currently no function to just modify the file stem

		output_path_tmp.set_file_name(stem);
	}

	// track if the image was converted and should be removed afterwards
	let mut is_tmp_image = false;
	// image path to a jpg image
	let image_path = {
		let tmp_dir = std::env::temp_dir().join("libytdlr-imageconvert");

		let image = image.as_ref();

		let converted = convert_image_to_jpg(image, tmp_dir)?;

		if converted != image {
			is_tmp_image = true;
		}

		converted
	};

	re_thumbnail(media, &image_path, &output_path_tmp)?;

	std::fs::rename(&output_path_tmp, output.as_ref()).attach_path_err(output_path_tmp)?;

	// remove temporary converted image file
	if is_tmp_image {
		std::fs::remove_file(&image_path).attach_path_err(image_path)?;
	}

	return Ok(());
}

/// Re-Apply a thumbnail from `image` onto `media` as `output`
/// Will not apply any image conversion
///
/// To Automatically handle with a temporary file, use [`re_thumbnail_with_tmp`]
pub fn re_thumbnail<M: AsRef<Path>, I: AsRef<Path>, O: AsRef<Path>>(
	media: M,
	image: I,
	output: O,
) -> Result<(), crate::Error> {
	let media = media.as_ref();
	let image = image.as_ref();
	let output = output.as_ref();

	info!(
		"ReThumbnail media \"{}\", with image \"{}\", into \"{}\"",
		media.to_string_lossy(),
		image.to_string_lossy(),
		output.to_string_lossy()
	);

	let ffmpeg_output = crate::spawn::ffmpeg::ffmpeg_probe(media)?;
	let container_formats = crate::spawn::ffmpeg::parse_format(&ffmpeg_output)?;

	if container_formats.contains(&"ogg") | container_formats.contains(&"flac") {
		return rethumbnail_ogg(media, image, output);
	}
	if container_formats.contains(&"matroska") {
		return rethumbnail_mkv(media, image, output);
	} else if container_formats.contains(&"mp3") {
		return rethumbnail_mp3_lofty(media, image, output);

		// return rethumbnail_mp3_ffmpeg(media, image, output);
	}

	return Err(crate::Error::other(format!(
		"Unhandled container format: \"{}\"",
		container_formats.join(", ")
	)));
}

/// Rethumbnail for container format "ogg" (using lofty)
fn rethumbnail_ogg(media: &Path, image: &Path, output: &Path) -> Result<(), crate::Error> {
	trace!("Using lofty ogg rethumbnail");

	debug!("WHAT {:#?}", (media, image, output));

	// ffmpeg somehow does not support embedding a mjpeg to a ogg/opus file, so we have to use lofty

	// get the existing metadata in the original file
	let mut tagged_file = Probe::open(media)
		.map_err(|err| crate::Error::other(format!("LoftyError: {}", err)))?
		.read()
		.map_err(|err| crate::Error::other(format!("LoftyError: {}", err)))?;

	// get the existing metadata, either from the primary tag format, or the first found
	let primary_tag = match tagged_file.primary_tag_mut() {
		Some(v) => v,
		None => tagged_file
			.first_tag_mut()
			.ok_or_else(|| crate::Error::other(format!("No tags in file \"{}\"", media.display())))?,
	};

	// read & add the picture
	let mut reader = BufReader::new(File::open(image).attach_path_err(image)?);
	let mut picture = Picture::from_reader(&mut reader)
		.map_err(|err| crate::Error::other(format!("Could not parse picture at \"{}\": {:#}", image.display(), err)))?;
	picture.set_pic_type(PictureType::CoverFront);
	// set picture instead of push to only have one image
	primary_tag.set_picture(0, picture);

	// copy the original file first, because lofty changes metadata and does not remux (requires existing file)
	// but dont apply it to the original yet
	std::fs::copy(media, output).attach_path_err(output)?;

	primary_tag
		.save_to_path(output, WriteOptions::default())
		.expect("Writing tags failed");

	Ok(())
}

/// Rethumbnail fo container format "mkv" and related
fn rethumbnail_mkv(media: &Path, image: &Path, output: &Path) -> Result<(), crate::Error> {
	trace!("Using ffmpeg mkv rethumbnail");
	let mut cmd = crate::spawn::ffmpeg::base_ffmpeg_hidebanner(true);

	cmd.arg("-i").arg(media); // set media file as input "0"

	// in mkv, covers should be attachments
	cmd.arg("-attach").arg(image);
	cmd.args([
		"-metadata:s:t:0",
		"mimetype=image/jpeg", // set the attachment's mimetype (because it is not automatically done)
		"-c",
		"copy", // copy everything instead of re-encoding
	]);

	cmd.arg(output); // set output path

	return re_thumbnail_with_command(cmd);
}

// the following code is retained in case it is ever necessary
// /// Rethumbnail fo container format "mp3"
// fn rethumbnail_mp3_ffmpeg(media: &Path, image: &Path, output: &Path) -> Result<(), crate::Error> {
// 	trace!("Using ffmpeg mp3 rethumbnail");
// 	let mut cmd = crate::spawn::ffmpeg::base_ffmpeg_hidebanner(true);

// 	cmd.arg("-i").arg(media); // set media file as input "0"
// 	cmd.arg("-i").arg(image); // set image file as input "1"

// 	cmd.args([
// 		"-map",
// 		"0", // map input stream 0 to output stream 0
// 		"-map",
// 		"1", // map input stream 1 to output stream 0
// 		"-c",
// 		"copy", // copy all input streams into output stream without re-encoding
// 		"-disposition:v:1",
// 		"attached_pic", // set input "1" as the thumbnail (required for some thumbnails, like mp4 - also works with others)
// 		"-id3v2_version",
// 		"3", // set which id3 version to use
// 		"-metadata:s:v",
// 		"title=\"Album cover\"", // set metadata for output video stream
// 	]);

// 	// the following options seem to not work correctly anymore
// 	// cmd.args([
// 	// 	"-movflags",
// 	// 	"use_metadata_tags", // copy existing metadata tags
// 	// ]);

// 	cmd.arg(output); // set output path

// 	return re_thumbnail_with_command(cmd);
// }

/// Rethumbnail for container format "mp3" (using lofty)
fn rethumbnail_mp3_lofty(media: &Path, image: &Path, output: &Path) -> Result<(), crate::Error> {
	trace!("Using lofty mp3 rethumbnail");

	// alternative path for mp3, use lofty without having to spawn ffmpeg

	// get the existing metadata in the original file
	let mut tagged_file = Probe::open(media)
		.map_err(|err| crate::Error::other(format!("LoftyError: {}", err)))?
		.read()
		.map_err(|err| crate::Error::other(format!("LoftyError: {}", err)))?;

	// get the existing metadata, either from the primary tag format, or the first found
	let primary_tag = match tagged_file.primary_tag_mut() {
		Some(v) => v,
		None => tagged_file
			.first_tag_mut()
			.ok_or_else(|| crate::Error::other(format!("No tags in file \"{}\"", media.display())))?,
	};

	// read & add the picture
	let mut reader = BufReader::new(File::open(image).attach_path_err(image)?);
	let mut picture = Picture::from_reader(&mut reader)
		.map_err(|err| crate::Error::other(format!("Could not parse picture at \"{}\": {:#}", image.display(), err)))?;
	picture.set_pic_type(PictureType::CoverFront);
	// set picture instead of push to only have one image
	primary_tag.set_picture(0, picture);

	// copy the original file first, because lofty changes metadata and does not remux (requires existing file)
	// but dont apply it to the original yet
	std::fs::copy(media, output).attach_path_err(output)?;

	primary_tag
		.save_to_path(output, WriteOptions::default())
		.expect("Writing tags failed");

	Ok(())
}

/// Run the provided command and log the stderr
fn re_thumbnail_with_command(mut cmd: std::process::Command) -> Result<(), crate::Error> {
	// create pipe for stderr, other stream are ignored
	// this is because ffmpeg only logs to stderr, where stdout is used for data piping
	cmd.stdout(Stdio::null()).stderr(Stdio::piped()).stdin(Stdio::null());

	let mut child = cmd.spawn().attach_location_err("ffmpeg spawn")?;

	let stderr_reader = BufReader::new(child.stderr.take().ok_or_else(|| {
		return crate::Error::custom_ioerror_location(
			std::io::ErrorKind::BrokenPipe,
			"Failed to get Child STDERR",
			"ffmpeg stderr take",
		);
	})?);

	// offload the stderr reader to a different thread to not block main
	let stderrreader_thread = std::thread::Builder::new()
		.name("ffmpeg stderr handler".to_owned())
		.spawn(|| {
			stderr_reader
				.lines()
				.filter_map(|v| return v.ok())
				.for_each(|line| log::info!("ffmpeg STDERR: {}", line));
		})
		.attach_location_err("ffmpeg stderr thread spawn")?;

	stderrreader_thread.join_err()?;

	let exit_status = child.wait().attach_path_err("ffmpeg wait")?;

	if !exit_status.success() {
		return Err(crate::spawn::ffmpeg::unsuccessfull_command_exit(
			exit_status,
			"Enable log level INFO for error",
		));
	}

	return Ok(());
}

// List of image extensions to try to find
// sorted based on how common it should be
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "png", "webp"];

/// Find a image based on the input's media_path
/// Returns [`Some`] with a path to the image found, otherwise [`None`] if none was found
pub fn find_image<MP: AsRef<Path>>(media_path: MP) -> Result<Option<PathBuf>, crate::Error> {
	let media_path = media_path.as_ref();

	if !media_path.exists() {
		return Err(crate::Error::custom_ioerror_path(
			std::io::ErrorKind::NotFound,
			"media_path does not exist!",
			media_path,
		));
	}

	if !media_path.is_file() {
		return Err(crate::Error::not_a_file("media_path is not a file!", media_path));
	}

	// test for all extensions in IMAGE_EXTENSIONS
	for test_ext in IMAGE_EXTENSIONS {
		let mut image_path = media_path.to_owned();
		image_path.set_extension(test_ext);

		// if file is found, return it
		if image_path.exists() {
			return Ok(Some(image_path));
		}
	}

	return Ok(None);
}

/// Convert "image_path" into "jpg" if possible with ffmpeg
/// This will need to be used to convert * to jpg for thumbnails (mainly from webp)
/// "output_dir" will be used when a conversion happens to store the converted file
/// Returns the converted image's path
pub fn convert_image_to_jpg<IP: AsRef<Path>, OP: AsRef<Path>>(
	image_path: IP,
	output_dir: OP,
) -> Result<PathBuf, crate::Error> {
	let cmd = crate::spawn::ffmpeg::base_ffmpeg_hidebanner(true);

	return convert_image_to_jpg_with_command(cmd, image_path, output_dir);
}

/// Convert "image_path" into "jpg" if possible with the provided command base
/// This will need to be used to convert * to jpg for thumbnails (mainly from webp)
/// "output_dir" will be used when a conversion happens to store the converted file
/// Returns the converted image's path
///
/// This function should not be called directly, use [`convert_image_to_jpg`] instead
pub fn convert_image_to_jpg_with_command<IP: AsRef<Path>, OP: AsRef<Path>>(
	mut cmd: std::process::Command,
	image_path: IP,
	output_dir: OP,
) -> Result<PathBuf, crate::Error> {
	let image_path = image_path.as_ref();
	let output_dir = output_dir.as_ref();

	if !image_path.exists() {
		return Err(crate::Error::custom_ioerror_path(
			std::io::ErrorKind::NotFound,
			"image_path does not exist!",
			image_path,
		));
	}

	if !image_path.is_file() {
		return Err(crate::Error::not_a_file("image_path is not a file!", image_path));
	}

	// check if the input path is already a jpg, if it is do not apply ffmpeg
	if let Some(ext) = image_path.extension() {
		if ext == OsStr::new("jpg") {
			return Ok(image_path.to_owned());
		}
	}

	if output_dir.exists() && !output_dir.is_dir() {
		return Err(crate::Error::not_a_directory(
			"output_dir exists but is not a directory!",
			output_dir,
		));
	}

	std::fs::create_dir_all(output_dir).attach_path_err(output_dir)?;

	let output_path = {
		let filename = image_path
			.file_name()
			.ok_or_else(|| return crate::Error::other("Expected image_path to have a filename"))?;
		let mut tmp_path = output_dir.join(filename);

		tmp_path.set_extension("jpg");

		tmp_path
	};

	// set the input image
	cmd.arg("-i").arg(image_path);

	// set the output path
	cmd.arg(&output_path);

	// create pipe for stderr, other stream are ignored
	// this is because ffmpeg only logs to stderr, where stdout is used for data piping
	cmd.stdout(Stdio::null()).stderr(Stdio::piped()).stdin(Stdio::null());

	let mut ffmpeg_child = cmd.spawn().attach_location_err("ffmpeg spawn")?;

	let stderr_reader = BufReader::new(ffmpeg_child.stderr.take().ok_or_else(|| {
		return crate::Error::custom_ioerror_location(
			std::io::ErrorKind::BrokenPipe,
			"Failed to get Child STDERR",
			"ffmpeg stderr take",
		);
	})?);

	// offload the stderr reader to a different thread to not block main
	let ffmpeg_child_stderr_thread = std::thread::Builder::new()
		.name("ffmpeg stderr handler".to_owned())
		.spawn(|| {
			stderr_reader
				.lines()
				.filter_map(|v| return v.ok())
				.for_each(|line| log::info!("ffmpeg STDERR: {}", line));
		})
		.attach_location_err("ffmpeg stderr thread spawn")?;

	let ffmpeg_child_exit_status = ffmpeg_child.wait().attach_path_err("ffmpeg wait")?;

	// wait until the stderr thread has exited
	ffmpeg_child_stderr_thread.join_err()?;

	if !ffmpeg_child_exit_status.success() {
		return Err(if let Some(code) = ffmpeg_child_exit_status.code() {
			crate::Error::command_unsuccessful(format!("ffmpeg_child exited with code: {code}"))
		} else {
			let signal = match ffmpeg_child_exit_status.signal() {
				Some(code) => code.to_string(),
				None => "None".to_owned(),
			};

			crate::Error::command_unsuccessful(format!("ffmpeg_child exited with signal: {signal}"))
		});
	}

	return Ok(output_path);
}

#[cfg(test)]
mod test {
	use super::*;
	use tempfile::{
		Builder as TempBuilder,
		TempDir,
	};

	fn create_dir(target: &'static str) -> (PathBuf, TempDir) {
		let testdir = TempBuilder::new()
			.prefix(&format!("ytdl-test-{target}-"))
			.tempdir()
			.expect("Expected a temp dir to be created");

		return (testdir.as_ref().to_owned(), testdir);
	}

	mod re_thumbnail {
		use super::*;

		#[test]
		#[ignore = "CI Install not present currently"]
		fn test_exit_status() {
			let mut fake_command = std::process::Command::new("sh");
			fake_command.args([
				"-c", // random exit code that is non-0
				"exit 1",
			]);

			let output = re_thumbnail_with_command(fake_command);

			assert!(output.is_err());

			assert_eq!(
				crate::Error::command_unsuccessful("FFMPEG did not successfully exit! Exit Code: 1"),
				output.expect_err("Expected Assert to test Result to be ERR")
			);
		}
	}

	mod find_image {
		use super::*;

		#[test]
		fn test_find_image_jpg() {
			let (workdir, _tempdir) = create_dir("findimage");

			let test_file_path = workdir.join("somefile.jpg");

			std::fs::File::create(&test_file_path).expect("Expected File::create to be successfull");

			let result = find_image(&test_file_path);

			assert!(result.is_ok());

			let result = result.expect("Expected is_ok assert to throw");
			assert_eq!(Some(test_file_path), result);
		}

		#[test]
		fn test_find_image_png() {
			let (workdir, _tempdir) = create_dir("findimage");

			let test_file_path = workdir.join("somefile.png");

			std::fs::File::create(&test_file_path).expect("Expected File::create to be successfull");

			let result = find_image(&test_file_path);

			assert!(result.is_ok());

			let result = result.expect("Expected is_ok assert to throw");
			assert_eq!(Some(test_file_path), result);
		}

		#[test]
		fn test_find_image_webp() {
			let (workdir, _tempdir) = create_dir("findimage");

			let test_file_path = workdir.join("somefile.webp");

			std::fs::File::create(&test_file_path).expect("Expected File::create to be successfull");

			let result = find_image(&test_file_path);

			assert!(result.is_ok());

			let result = result.expect("Expected is_ok assert to throw");
			assert_eq!(Some(test_file_path), result);
		}
	}

	mod convert_image_to_jpg {
		use super::*;

		#[test]
		fn test_basic_func() {
			let (workdir, _tempdir) = create_dir("convertimagejpg");
			let fake_command = std::process::Command::new("echo");

			let output_dir = workdir.join("tmp");
			let image_path = workdir.join("hello.webp");
			std::fs::File::create(&image_path).expect("Expected File::create to be successfull");
			let expected_output = output_dir.join("hello.jpg");

			let result = convert_image_to_jpg_with_command(fake_command, image_path, output_dir);

			assert!(result.is_ok());
			let result = result.expect("Expected is_ok assert to throw");
			assert_eq!(&expected_output, &result);
		}

		#[test]
		fn test_early_return_jpg() {
			let (workdir, _tempdir) = create_dir("convertimagejpg");
			let output_dir = workdir.join("tmp");
			let mut fake_command = std::process::Command::new("sh");
			fake_command.args([
				"-c", // random exit code that is non-0
				"exit 1",
			]);

			let image_path = workdir.join("hello.jpg");
			std::fs::File::create(&image_path).expect("Expected File::create to be successfull");
			let expected_output = image_path.clone();

			let result = convert_image_to_jpg_with_command(fake_command, image_path, output_dir);

			assert!(result.is_ok());
			let result = result.expect("Expected is_ok assert to throw");
			assert_eq!(&expected_output, &result);
		}

		#[test]
		fn test_exit_status() {
			let (workdir, _tempdir) = create_dir("convertimagejpg");
			let output_dir = workdir.join("tmp");
			let mut fake_command = std::process::Command::new("sh");
			fake_command.args([
				"-c", // random exit code that is non-0
				"exit 1",
			]);

			let image_path = workdir.join("hello.webp");
			std::fs::File::create(&image_path).expect("Expected File::create to be successfull");

			let result = convert_image_to_jpg_with_command(fake_command, image_path, output_dir);

			assert!(result.is_err());

			assert_eq!(
				crate::Error::command_unsuccessful("ffmpeg_child exited with code: 1"),
				result.expect_err("Expected Assert to test Result to be ERR")
			);
		}
	}
}
