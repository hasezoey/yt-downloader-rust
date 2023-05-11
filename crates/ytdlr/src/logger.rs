//! Module for all Logger related things

use colored::{
	Color,
	Colorize,
};
use flexi_logger::{
	style,
	DeferredNow,
	Logger,
	LoggerHandle,
	Record,
};
use std::io::Error as ioError;

/// Function for setting up the logger
/// This function is mainly to keep the code structured and sorted
#[inline]
pub fn setup_logger() -> Result<LoggerHandle, ioError> {
	let handle = Logger::try_with_env_or_str("warn")
		.expect("Expected flexi_logger to be able to parse env or string")
		.adaptive_format_for_stderr(flexi_logger::AdaptiveFormat::Custom(log_format, color_log_format))
		.log_to_stderr()
		.start()
		.expect("Expected flexi_logger to be able to start");

	return Ok(handle);
}

/// Logging format for log files and non-interactive formats
/// Not Colored and not padded
///
/// Example Lines:
/// `[2022-03-02T13:42:43.374+0100 ERROR module]: test line`
/// `[2022-03-02T13:42:43.374+0100 WARN module::deeper]: test line`
pub fn log_format(w: &mut dyn std::io::Write, now: &mut DeferredNow, record: &Record) -> Result<(), std::io::Error> {
	return write!(
		w,
		"[{} {} {}]: {}", // dont pad anything for non-interactive logs
		now.format_rfc3339(),
		record.level(),
		record.module_path().unwrap_or("<unnamed module>"),
		&record.args()
	);
}

/// Logging format for a tty for interactive formats
/// Colored and padded
///
/// Example Lines:
/// `[2022-03-02T13:42:43.374+0100 ERROR module]: test line`
/// `[2022-03-02T13:42:43.374+0100 WARN  module::deeper]: test line`
pub fn color_log_format(
	w: &mut dyn std::io::Write,
	now: &mut DeferredNow,
	record: &Record,
) -> Result<(), std::io::Error> {
	let level = record.level();
	return write!(
		w,
		"[{} {} {}]: {}",
		now.format_rfc3339().to_string().color(Color::BrightBlack), // Bright Black = Grey
		style(level).paint(format!("{level:5}")), // pad level to 2 characters, cannot be done in the string itself, because of the color characters
		record.module_path().unwrap_or("<unnamed module>"),
		&record.args() // dont apply any color to the input, so that the input can dynamically set the color
	);
}
