use serde::{Deserialize, Serialize};

use crate::GeneratedImage;

/// Result of a non-streaming image-generation request.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct GenerateImageResult {
    /// Request-level seed actually sent to `NovelAI` after resolving an input seed of `0`.
    pub resolved_seed: i64,
    /// Images decoded from the response.
    pub images: Vec<GeneratedImage>,
}

/// Parsed stream chunk emitted by `NovelAI` image streaming.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageStreamChunk {
    /// Stream event type.
    pub event_type: ImageStreamEventType,
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

impl std::fmt::Debug for ImageStreamChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageStreamChunk")
            .field("event_type", &self.event_type)
            .field("samp_ix", &self.samp_ix)
            .field("step_ix", &self.step_ix)
            .field("gen_id", &self.gen_id)
            .field("sigma", &self.sigma)
            .field("image_bytes", &self.image.len())
            .finish_non_exhaustive()
    }
}

/// Stream event identity, preserving provider additions without inventing local semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageStreamEventType {
    /// A sampling step image.
    Step,
    /// An unrecognized provider event.
    Unknown(String),
}

impl ImageStreamEventType {
    /// Provider event identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Step => "step",
            Self::Unknown(value) => value,
        }
    }
}

impl From<String> for ImageStreamEventType {
    fn from(value: String) -> Self {
        if value == "step" {
            Self::Step
        } else {
            Self::Unknown(value)
        }
    }
}

impl From<&str> for ImageStreamEventType {
    fn from(value: &str) -> Self { value.to_owned().into() }
}

impl Serialize for ImageStreamEventType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ImageStreamEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Into::into)
    }
}

#[cfg(test)]
mod tests;
