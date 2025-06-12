use std::sync::LazyLock;

use crate::{
	clap_conf::{
		ArchiveImport,
		CliDerive,
	},
	utils,
};
use indicatif::{
	ProgressBar,
	ProgressStyle,
};
use libytdlr::main::archive::import::{
	import_any_archive,
	ImportProgress,
};

/// Handler function for the "archive import" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_import(main_args: &CliDerive, sub_args: &ArchiveImport) -> Result<(), crate::Error> {
	println!("Importing Archive from \"{}\"", sub_args.file_path.to_string_lossy());

	let input_path = &sub_args.file_path;

	let archive_path = match main_args.archive_path.as_ref() {
		None => return Err(crate::Error::other("Archive is required for Import!")),
		Some(v) => v,
	};

	static IMPORT_STYLE: LazyLock<ProgressStyle> = LazyLock::new(|| {
		return ProgressStyle::default_bar()
			.template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
			.expect("Expected ProgressStyle template to be valid")
			.progress_chars("#>-");
	});

	let bar: ProgressBar = ProgressBar::hidden().with_style(IMPORT_STYLE.clone());
	crate::utils::set_progressbar(&bar, main_args);

	let (_new_archive, mut connection) = utils::handle_connect(archive_path, &bar, main_args)?;

	let pgcb_import = |imp| {
		if main_args.is_interactive() {
			match imp {
				ImportProgress::Starting => bar.set_position(0),
				ImportProgress::SizeHint(v) => bar.set_length(v.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Increase(c, _i) => bar.inc(c.try_into().expect("Failed to convert usize to u64")),
				ImportProgress::Finished(v) => bar.finish_with_message(format!("Finished Importing {v} elements")),
			}
		} else {
			match imp {
				ImportProgress::Starting => println!("Starting Import"),
				ImportProgress::SizeHint(v) => println!("Import SizeHint: {v}"),
				ImportProgress::Increase(c, i) => println!("Import Increase: {c}, Current Index: {i}"),
				ImportProgress::Finished(v) => println!("Import Finished, Successfull Imports: {v}"),
			}
		}
	};

	import_any_archive(input_path, &mut connection, pgcb_import)?;

	return Ok(());
}
