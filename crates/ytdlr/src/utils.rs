//! Utils for the `ytdlr` binary

use crate::clap_conf::*;
use indicatif::{
	ProgressBar,
	ProgressDrawTarget,
};

/// Helper function to set the progressbar to a draw target if mode is interactive
pub fn set_progressbar(bar: &ProgressBar, main_args: &CliDerive) -> () {
	if main_args.is_interactive() {
		bar.set_draw_target(ProgressDrawTarget::stderr());
	}
}
