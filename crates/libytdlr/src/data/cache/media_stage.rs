//! Module for [`MediaStage`]

use serde::{
	Deserialize,
	Serialize,
};

/// Enum Representing the Stages for Media processing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediaStage {
	/// Variant representing that the no stage has been started yet
	None,
	/// Variant representing that the "Downloading" stage has been done / is currently is process
	Downloading,
	/// Variant representing that the "PostProcessing" stage has been done / is currently is process
	PostProcessing,
}

impl Default for MediaStage {
	fn default() -> Self {
		return Self::None;
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_default() {
		assert_eq!(MediaStage::None, MediaStage::default());
	}
}
