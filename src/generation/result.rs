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

/// Parsed provider event together with its complete SSE source.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ImageStreamChunk {
    pub event: ImageStreamEvent,
    /// Original data, including fields not interpreted by this version of the library.
    pub raw: TransportSseEvent,
}

/// Complete SSE fields exposed by custom transports and retained on parsed events/errors.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransportSseEvent {
    /// SSE event name; empty or `message` means the JSON discriminator selects the type.
    pub event: String,
    pub id: String,
    pub data: String,
    pub retry: Option<std::time::Duration>,
}

impl std::fmt::Debug for TransportSseEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportSseEvent")
            .field("event", &self.event)
            .field("data_bytes", &self.data.len())
            .finish_non_exhaustive()
    }
}

/// Official image event variants. Unknown events never assume the shape of an image event.
#[derive(Clone, Serialize, PartialEq)]
pub enum ImageStreamEvent {
    Intermediate {
        samp_ix: u32,
        step_ix: u32,
        gen_id: u32,
        sigma: Option<f32>,
        /// Base64-encoded preview image.
        image: String,
    },
    Final {
        samp_ix: u32,
        gen_id: u32,
        /// A complete, validated image rather than a preview or payload fragment.
        image: GeneratedImage,
    },
    Error {
        samp_ix: Option<u32>,
        gen_id: Option<u32>,
        message: String,
    },
    /// The complete uninterpreted event is available through `ImageStreamChunk::raw`.
    Unknown { event_type: String },
}

impl ImageStreamChunk {
    #[must_use]
    pub fn event_type(&self) -> ImageStreamEventType {
        match &self.event {
            ImageStreamEvent::Intermediate { .. } => ImageStreamEventType::Intermediate,
            ImageStreamEvent::Final { .. } => ImageStreamEventType::Final,
            ImageStreamEvent::Error { .. } => ImageStreamEventType::Error,
            ImageStreamEvent::Unknown { event_type } => {
                ImageStreamEventType::Unknown(event_type.clone())
            },
        }
    }

    #[must_use]
    pub const fn sample_index(&self) -> Option<u32> {
        match &self.event {
            ImageStreamEvent::Intermediate { samp_ix, .. }
            | ImageStreamEvent::Final { samp_ix, .. } => Some(*samp_ix),
            ImageStreamEvent::Error { samp_ix, .. } => *samp_ix,
            ImageStreamEvent::Unknown { .. } => None,
        }
    }
}

impl std::fmt::Debug for ImageStreamEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Intermediate {
                samp_ix,
                step_ix,
                gen_id,
                sigma,
                image,
            } => f
                .debug_struct("Intermediate")
                .field("samp_ix", samp_ix)
                .field("step_ix", step_ix)
                .field("gen_id", gen_id)
                .field("sigma", sigma)
                .field("image_bytes", &image.len())
                .finish(),
            Self::Final {
                samp_ix,
                gen_id,
                image,
            } => f
                .debug_struct("Final")
                .field("samp_ix", samp_ix)
                .field("gen_id", gen_id)
                .field("image", image)
                .finish(),
            Self::Error {
                samp_ix, gen_id, ..
            } => f
                .debug_struct("Error")
                .field("samp_ix", samp_ix)
                .field("gen_id", gen_id)
                .finish_non_exhaustive(),
            Self::Unknown { event_type } => f
                .debug_struct("Unknown")
                .field("event_type", event_type)
                .finish(),
        }
    }
}

/// Provider identity, including genuinely unknown extensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageStreamEventType {
    Intermediate,
    Final,
    Error,
    Unknown(String),
}

impl ImageStreamEventType {
    /// Provider event identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Intermediate => "intermediate",
            Self::Final => "final",
            Self::Error => "error",
            Self::Unknown(value) => value,
        }
    }
}

impl From<String> for ImageStreamEventType {
    fn from(value: String) -> Self {
        match value.as_str() {
            "intermediate" => Self::Intermediate,
            "final" => Self::Final,
            "error" => Self::Error,
            _ => Self::Unknown(value),
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
