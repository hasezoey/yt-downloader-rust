use std::path::PathBuf;

use crate::{
	clap_conf::{
		CliDerive,
		CommandReThumbnail,
	},
	utils,
};
use libytdlr::main::rethumbnail::re_thumbnail_with_tmp;

/// Handler function for the "rethumbnail" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_rethumbnail(_main_args: &CliDerive, sub_args: &CommandReThumbnail) -> Result<(), crate::Error> {
	utils::require_ffmpeg_installed()?;

	// helper aliases to make it easier to access
	let input_image_path: &PathBuf = &sub_args.input_image_path;
	let input_media_path: &PathBuf = &sub_args.input_media_path;
	let output_media_path: &PathBuf = sub_args
		.output_media_path
		.as_ref()
		.expect("Expected trait \"Check\" to be run on \"CommandReThumbnail\" before this point");

	println!(
		"Re-Applying Thumbnail image \"{}\" to media file \"{}\"",
		input_image_path.to_string_lossy(),
		input_media_path.to_string_lossy()
	);

	re_thumbnail_with_tmp(input_media_path, input_image_path, output_media_path)?;

	println!(
		"Re-Applied Thumbnail to media, as \"{}\"",
		output_media_path.to_string_lossy()
	);

	return Ok(());
}
