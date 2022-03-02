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
use time::{
	format_description::FormatItem,
	macros::format_description,
};

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

/// ISO 8601 Time Format for logging
pub const ISO8601_TIME_FORMAT: &[FormatItem<'static>] = format_description!(
	// format to be "1977-11-30T13:30:30.000+0200"
	"[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3][offset_hour sign:mandatory][offset_minute]"
);

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
		now.format(ISO8601_TIME_FORMAT),
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
		now.format(ISO8601_TIME_FORMAT).color(Color::BrightBlack), // Bright Black = Grey
		style(level).paint(format!("{:5}", level)), // pad level to 2 characters, cannot be done in the string itself, because of the color characters
		record.module_path().unwrap_or("<unnamed module>"),
		&record.args() // dont apply any color to the input, so that the input can dynamically set the color
	);
}
