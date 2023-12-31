use crate::{
	clap_conf::{
		CliDerive,
		CommandUnicodeTerminalTest,
	},
	utils::msg_to_cluster,
};

/// Handler function for the "unicode_test" subcommand
/// This function is mainly to keep the code structured and sorted
///
/// There are a lot of unicode and terminal problems, for example a wcwidth and wcswidth mismatch, or some terminals deciding some character is 2 wide instead of 1,
/// this command exists to find to find and debug those kinds of problems more easily
#[inline]
pub fn command_unicodeterminaltest(
	_main_args: &CliDerive,
	sub_args: &CommandUnicodeTerminalTest,
) -> Result<(), crate::Error> {
	let msg = &sub_args.string;
	println!("Unicode Terminal Test");

	// dont run anything on a empty message, because it makes the code below a lot easier with less cases (and 0 width evaluation is likely not needed)
	if msg.is_empty() {
		return Err(crate::Error::other("input is empty, not running any tests!"));
	}

	println!("Message (raw):\n{:#?}", msg);
	println!("Message (printed):\n{}", msg);

	let details = msg_to_cluster(&msg);
	let last_char = details.last().expect("Expected to return earlier at this case");
	println!("{}^", (1..last_char.display_pos).map(|_| ' ').collect::<String>());
	println!(
		"App thinks display-width is {}, above \"^\" means where it thinks in terms of display",
		last_char.display_pos,
	);

	if sub_args.print_content {
		println!("msg_to_cluster array:\n{:#?}", details);
	} else {
		println!("msg_to_cluster array: not enabled (use -c)")
	}

	return Ok(());
}
