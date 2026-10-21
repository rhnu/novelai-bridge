use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Indicates where a vibe metadata entry was recovered from.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum VibeMetadataSource {
    /// Read from the `NovelAI_Vibe_Encoding_Base64` PNG text chunk.
    NovelAiVibeEncodingChunk,
    /// Read from the `naidata` PNG text chunk.
    NaiDataChunk,
    /// Read from the legacy `naiv4vibe` PNG text chunk.
    LegacyVibeChunk,
    /// Reconstructed from the JSON payload stored in the `Comment` chunk.
    CommentPayload,
}

/// A single vibe-related metadata entry recovered from a PNG payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VibeMetadataEntry {
    /// The source that produced this entry.
    pub source: VibeMetadataSource,
    /// Optional display name if present in the metadata.
    pub display_name: Option<String>,
    /// Primary base64-encoded vibe payload when available.
    pub encoding: Option<String>,
    /// Additional model-specific encodings keyed by model identifier.
    pub model_encodings: BTreeMap<String, String>,
    /// Optional strength value associated with the vibe.
    pub strength: Option<f32>,
    /// Optional information-extracted value associated with the vibe.
    pub information_extracted: Option<f32>,
    /// Whether the metadata entry references an embedded image or thumbnail.
    pub has_image: bool,
}

/// Recoverable issue encountered while interpreting PNG text metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PngMetadataWarning {
    /// A non-empty `Comment` text chunk was present but was not valid supported JSON.
    InvalidCommentJson,
    /// A compressed PNG text chunk could not be decoded within the supported limit.
    InvalidTextChunk {
        /// PNG text chunk keyword.
        keyword: String,
        /// Decoder diagnostic.
        message: String,
    },
}

/// Parsed metadata recovered from a PNG image payload.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PngMetadata {
    /// Prompt text when present.
    pub prompt: Option<String>,
    /// Negative prompt text when present.
    pub negative_prompt: Option<String>,
    /// Seed value when present.
    pub seed: Option<i64>,
    /// Sampler name when present.
    pub sampler: Option<String>,
    /// Step count when present.
    pub steps: Option<u32>,
    /// CFG scale when present.
    pub scale: Option<f32>,
    /// Image width when present.
    pub width: Option<u32>,
    /// Image height when present.
    pub height: Option<u32>,
    /// Model identifier when present.
    pub model: Option<String>,
    /// Noise schedule when present.
    pub noise_schedule: Option<String>,
    /// CFG rescale value when present.
    pub cfg_rescale: Option<f32>,
    /// UC preset identifier when present.
    pub uc_preset: Option<i32>,
    /// Quality toggle flag when present.
    pub quality_toggle: Option<bool>,
    /// Source text chunk value when present.
    pub source: Option<String>,
    /// Software text chunk value when present.
    pub software: Option<String>,
    /// Extracted V4 character prompts.
    pub character_prompts: Vec<String>,
    /// Extracted V4 negative character prompts.
    pub character_negative_prompts: Vec<String>,
    /// Recovered vibe metadata entries.
    pub vibe_entries: Vec<VibeMetadataEntry>,
    /// All text chunks grouped by key.
    pub text_chunks: BTreeMap<String, Vec<String>>,
    /// Parsed JSON payload from the `Comment` chunk when available.
    pub raw_comment_json: Option<Value>,
    /// Recoverable interpretation issues; successfully decoded text chunks remain available.
    pub warnings: Vec<PngMetadataWarning>,
}

impl PngMetadata {
    /// Returns whether the PNG contained no decoded text chunks or text diagnostics.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.text_chunks.is_empty() && self.warnings.is_empty() }

    /// Returns whether at least one recognized generation setting was recovered.
    #[must_use]
    pub const fn has_generation_data(&self) -> bool {
        self.prompt.is_some()
            || self.negative_prompt.is_some()
            || self.seed.is_some()
            || self.sampler.is_some()
            || self.steps.is_some()
            || self.scale.is_some()
            || self.width.is_some()
            || self.height.is_some()
            || self.model.is_some()
            || self.noise_schedule.is_some()
            || self.cfg_rescale.is_some()
            || self.uc_preset.is_some()
            || self.quality_toggle.is_some()
            || !self.character_prompts.is_empty()
            || !self.character_negative_prompts.is_empty()
    }
}

/// Best-effort metadata inspection state for one generated image.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum GeneratedImageMetadata {
    /// PNG metadata was parsed successfully.
    Parsed(Box<PngMetadata>),
    /// The payload is a valid PNG but contains no text metadata.
    NotPresent,
    /// The image format does not use the supported `NovelAI` PNG metadata representation.
    UnsupportedFormat,
    /// The payload looked like PNG data but its metadata could not be decoded.
    Invalid {
        /// Human-readable parser diagnostic.
        message: String,
    },
}

impl GeneratedImageMetadata {
    /// Returns parsed PNG metadata when available.
    #[must_use]
    pub fn parsed(&self) -> Option<&PngMetadata> {
        match self {
            Self::Parsed(metadata) => Some(metadata.as_ref()),
            Self::NotPresent | Self::UnsupportedFormat | Self::Invalid { .. } => None,
        }
    }

    /// Returns the per-image seed recovered from embedded metadata.
    #[must_use]
    pub fn seed(&self) -> Option<i64> { self.parsed().and_then(|metadata| metadata.seed) }
}
