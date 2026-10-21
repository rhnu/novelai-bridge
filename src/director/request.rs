use serde::{Deserialize, Serialize};

use crate::{EncodedImage, ValidationPolicy};

/// Valid director operation and its operation-specific parameters.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct RunDirectorToolRequest {
    /// Validated source image.
    pub image: EncodedImage,
    /// Operation and its valid parameter set.
    pub command: DirectorCommand,
    /// Rejects out-of-range numeric values instead of clamping them.
    #[serde(default)]
    pub policy: ValidationPolicy,
}

impl std::fmt::Debug for DirectorCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut view = f.debug_struct("DirectorCommand");
        view.field("operation", &self.wire_name());
        match self {
            Self::Emotion { prompt, defry } => {
                view.field("prompt_chars", &prompt.chars().count())
                    .field("defry", defry);
            },
            Self::Colorize { prompt, defry } => {
                view.field(
                    "prompt_chars",
                    &prompt.as_ref().map(|text| text.chars().count()),
                )
                .field("defry", defry);
            },
            _ => {},
        }
        view.finish_non_exhaustive()
    }
}

impl DirectorCommand {
    pub(crate) const fn wire_name(&self) -> &'static str {
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
