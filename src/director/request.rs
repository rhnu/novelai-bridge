use serde::{Deserialize, Serialize};

use crate::EncodedImage;

/// Valid director operation and its operation-specific parameters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DirectorCommand {
    /// Line art extraction.
    Lineart,
    /// Sketch conversion.
    Sketch,
    /// Background removal.
    BgRemoval,
    /// Emotion transfer. The prompt is required by this operation.
    Emotion {
        /// Emotion prompt.
        prompt: String,
        /// Defry value in the `[0, 5]` range.
        defry: u8,
    },
    /// Declutter cleanup.
    Declutter,
    /// Colorization with an optional guidance prompt.
    Colorize {
        /// Optional colorization prompt.
        prompt: Option<String>,
        /// Defry value in the `[0, 5]` range.
        defry: u8,
    },
}

/// Editable input for a director operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunDirectorToolRequest {
    /// Validated source image.
    pub image: EncodedImage,
    /// Operation and its valid parameter set.
    pub command: DirectorCommand,
    /// Rejects out-of-range numeric values instead of clamping them.
    pub strict_mode: bool,
}

impl DirectorCommand {
    pub(super) const fn wire_name(&self) -> &'static str {
        match self {
            Self::Lineart => "lineart",
            Self::Sketch => "sketch",
            Self::BgRemoval => "bg-removal",
            Self::Declutter => "declutter",
            Self::Colorize { .. } => "colorize",
            Self::Emotion { .. } => "emotion",
        }
    }
}
