use crate::clap_conf::*;
use crate::utils;
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use std::{
	fs::File,
	io::{
		BufReader,
		Error as ioError,
	},
};

/// Handler function for the "archive import" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_import(main_args: &CliDerive, sub_args: &ArchiveImport) -> Result<(), ioError> {
	use libytdlr::main::archive::import::*;
	println!("Importing Archive from \"{}\"", sub_args.file_path.to_string_lossy());

	let input_path = &sub_args.file_path;

	if main_args.archive_path.is_none() {
		return Err(ioError::new(
			std::io::ErrorKind::Other,
			"Archive is required for Import!",
		));
	}

	let archive_path = main_args
		.archive_path
		.as_ref()
		.expect("Expected archive check to have already returned");

	lazy_static::lazy_static! {
		static ref IMPORT_STYLE: ProgressStyle = ProgressStyle::default_bar()
			.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
			.expect("Expected ProgressStyle template to be valid")
			.progress_chars("#>-");
	}

	let bar: ProgressBar = ProgressBar::hidden().with_style(IMPORT_STYLE.clone());
	crate::utils::set_progressbar(&bar, main_args);

	let (_new_archive, mut connection) = utils::handle_connect(archive_path, &bar, main_args)?;

	let mut reader = BufReader::new(File::open(input_path)?);

	let pgcb_import = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {v} elements")),
				_ => (),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Import"),
				ImportProgress::SizeHint(v) => println!("Import SizeHint: {v}"),
				ImportProgress::Increase(c, i) => println!("Import Increase: {c}, Current Index: {i}"),
				ImportProgress::Finished(v) => println!("Import Finished, Successfull Imports: {v}"),
				_ => (),
			}
		}
	};

	import_any_archive(&mut reader, &mut connection, pgcb_import)?;

	return Ok(());
}
