use serde::{Deserialize, Serialize};
use crate::GeneratedImage;

/// Result of a non-streaming image-generation request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenerateImageResult {
    /// Request-level seed actually sent to `NovelAI` after resolving an input seed of `0`.
    pub resolved_seed: i64,
    /// Images decoded from the response.
    pub images: Vec<GeneratedImage>,
}

/// Parsed stream chunk emitted by `NovelAI` image streaming.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageStreamChunk {
    /// Stream event type.
    pub event_type: String,
    /// Sample index.
    pub samp_ix: u32,
    /// Optional step index.
    pub step_ix: Option<u32>,
    /// Generation identifier.
    pub gen_id: u32,
    /// Optional sigma value.
    pub sigma: Option<f32>,
    /// Base64-encoded image payload fragment.
    pub image: String,
}
