use serde::{Deserialize, Serialize};

use crate::{EncodedImage, Model, VibeEncoding};

/// One vibe transfer reference.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VibeReference {
    /// Precomputed payload produced by the vibe-encoding operation.
    pub vibe_data_cache: VibeEncoding,
    /// Per-image reference strength.
    pub strength: f32,
}

/// Vibe transfer configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VibeTransferConfig {
    /// One or more precomputed vibe payloads.
    pub references: Vec<VibeReference>,
    /// Overall vibe transfer strength.
    pub strength: f32,
}

/// Editable input for a vibe-encoding operation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncodeVibeRequest {
    /// Validated source image.
    pub image: EncodedImage,
    /// Information-extracted value in the `[0.01, 1.0]` range.
    pub information_extracted: f32,
    /// Target model for the encoded vibe payload.
    pub model: Model,
    /// Rejects out-of-range numeric values instead of clamping them.
    pub strict_mode: bool,
}
