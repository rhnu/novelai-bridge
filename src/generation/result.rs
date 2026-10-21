use serde::Serialize;

use crate::GeneratedImage;

/// Result of a non-streaming image-generation request.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct GenerateImageResult {
    /// Request-level seed actually sent to `NovelAI` after resolving an input seed of `0`.
    pub resolved_seed: i64,
    /// Images decoded from the response.
    pub images: Vec<GeneratedImage>,
}
