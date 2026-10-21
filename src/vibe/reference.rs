use serde::{Deserialize, Serialize};

use crate::{EncodedImage, VibeEncoding};

/// A model-specific vibe input: V3 accepts an image, later families accept encodings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VibeReferenceInput {
    /// Precomputed payload produced by the encode-vibe operation.
    Encoded { encoding: VibeEncoding },
    /// An original image and the amount of information extracted from it.
    Image {
        image: EncodedImage,
        #[serde(default = "full_information")]
        information_extracted: f32,
    },
}

/// One vibe transfer reference.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VibeReference {
    /// Input required by this model's vibe profile.
    pub input: VibeReferenceInput,
    /// Finite, signed per-reference strength.
    pub strength: f32,
}

impl VibeReference {
    /// Creates a reference from an existing encoding.
    #[must_use]
    pub const fn encoded(encoding: VibeEncoding, strength: f32) -> Self {
        Self {
            input: VibeReferenceInput::Encoded { encoding },
            strength,
        }
    }

    /// Creates a V3 image reference. Information extraction is normalized to 0–1.
    #[must_use]
    pub const fn image(image: EncodedImage, information_extracted: f32, strength: f32) -> Self {
        Self {
            input: VibeReferenceInput::Image {
                image,
                information_extracted,
            },
            strength,
        }
    }
}

/// Vibe transfer configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VibeTransferConfig {
    /// References using the input path required by the selected model.
    pub references: Vec<VibeReference>,
    /// Normalize encoded-reference strengths when their absolute sum exceeds one.
    /// Raw image references do not use this setting.
    #[serde(default = "enabled")]
    pub normalize_strength: bool,
}

impl VibeTransferConfig {
    /// Creates a configuration with encoded-strength normalization enabled.
    #[must_use]
    pub const fn new(references: Vec<VibeReference>) -> Self {
        Self {
            references,
            normalize_strength: true,
        }
    }
}

const fn enabled() -> bool { true }

const fn full_information() -> f32 { 1.0 }

#[cfg(test)]
mod tests;
