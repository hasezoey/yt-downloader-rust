use std::path::PathBuf;

use libytdlr::main::rethumbnail::re_thumbnail_with_tmp;

fn main() -> Result<(), libytdlr::Error> {
	let mut args = std::env::args();

	let _ = args.next();

	let media_path = PathBuf::from(args.next().expect("Expected First argument to be for the media file"));
	let image_path = PathBuf::from(args.next().expect("Expected Second argument to be for the image file"));
	let output_path = PathBuf::from(args.next().expect("Expected Third argument to be for the output file"));

	re_thumbnail_with_tmp(&media_path, &image_path, &output_path)?;

	println!("Done");

	Ok(())
}
