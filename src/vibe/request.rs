use serde::{Deserialize, Serialize};

use crate::{EncodedImage, Model, ValidationPolicy};

/// Editable input for a vibe-encoding operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EncodeVibeRequest {
    /// Validated source image.
    pub image: EncodedImage,
    /// Information-extracted value in the `[0, 1]` range.
    pub information_extracted: f32,
    /// Target model for the encoded vibe payload.
    pub model: Model,
    /// Optional source mask, fitted to the source image dimensions during preparation.
    #[serde(default)]
    pub mask: Option<EncodedImage>,
    /// Asks the service to crop to the visible mask bounds. `Some(true)` requires a mask.
    #[serde(default)]
    pub crop_to_mask: Option<bool>,
    /// Seed for choosing source regions; omitted values use the provider's content-based seed.
    #[serde(default)]
    pub focus_seed: Option<i64>,
    /// Seed for information noising; omitted values use the provider's content-based seed.
    #[serde(default)]
    pub info_extract_seed: Option<i64>,
    /// Rejects out-of-range numeric values instead of clamping them.
    #[serde(default)]
    pub policy: ValidationPolicy,
}

impl EncodeVibeRequest {
    /// Creates an encoding request with full information extraction and normalization enabled.
    #[must_use]
    pub const fn new(image: EncodedImage, model: Model) -> Self {
        Self {
            image,
            information_extracted: 1.0,
            model,
            mask: None,
            crop_to_mask: None,
            focus_seed: None,
            info_extract_seed: None,
            policy: ValidationPolicy::Normalize,
        }
    }
}
