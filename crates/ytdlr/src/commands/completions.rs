use std::io::{
	BufWriter,
	Write,
};

use clap::CommandFactory;
use clap_complete::generate;
use libytdlr::error::IOErrorToError;

use crate::clap_conf::{
	CliDerive,
	CommandCompletions,
};

/// Handler function for the "completions" subcommand
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn command_completions(_main_args: &CliDerive, sub_args: &CommandCompletions) -> Result<(), crate::Error> {
	let mut writer: BufWriter<Box<dyn Write>> = match &sub_args.output_file_path {
		Some(v) => {
			if v.exists() {
				return Err(crate::Error::other("Output file already exists"));
			}
			let v_parent = v.parent().expect("Expected input filename to have a parent");
			std::fs::create_dir_all(v_parent).attach_path_err(v_parent)?;
			BufWriter::new(Box::from(std::fs::File::create(v).attach_path_err(v)?))
		},
		None => BufWriter::new(Box::from(std::io::stdout())),
	};
	let mut parsed = CliDerive::command();
	let bin_name = parsed
		.get_bin_name()
		.expect("Expected binary to have a binary name")
		.to_string();
	generate(sub_args.shell, &mut parsed, bin_name, &mut writer);

	return Ok(());
}
