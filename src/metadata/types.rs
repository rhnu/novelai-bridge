use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The selected metadata carrier. Generation settings never merge across carriers.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataOrigin {
    PngText,
    ExifUserComment,
    StealthPngComp,
}

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
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeMetadataEntry {
    /// The source that produced this entry.
    pub source: VibeMetadataSource,
    /// Optional display name if present in the metadata.
    pub display_name: Option<String>,
    /// Primary base64-encoded vibe payload when available.
    pub encoding: Option<String>,
    /// Encodings keyed by model identifier, then by the encoding-parameter cache key.
    pub model_encodings: BTreeMap<String, BTreeMap<String, String>>,
    /// Optional strength value associated with the vibe.
    pub strength: Option<f32>,
    /// Optional information-extracted value associated with the vibe.
    pub information_extracted: Option<f32>,
    /// Whether the metadata entry references an embedded image or thumbnail.
    pub has_image: bool,
}

impl std::fmt::Debug for VibeMetadataEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeMetadataEntry")
            .field("source", &self.source)
            .field("has_encoding", &self.encoding.is_some())
            .field("model_encoding_count", &self.model_encodings.len())
            .field("has_image", &self.has_image)
            .finish_non_exhaustive()
    }
}

/// Recoverable issue encountered while interpreting image metadata.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetadataWarning {
    /// A recognized carrier could not be decoded within its metadata budget.
    InvalidCarrier {
        origin: MetadataOrigin,
        message: String,
    },
    /// A non-empty `Comment` text chunk was present but was not valid supported JSON.
    InvalidCommentJson {
        /// Complete parser diagnostic without retaining the comment payload.
        message: String,
    },
    /// A recognized numeric field could not be represented as a finite value.
    InvalidNumericValue {
        field: MetadataField,
        message: String,
    },
    /// A compressed PNG text chunk could not be decoded within the supported limit.
    InvalidTextChunk {
        /// PNG text chunk keyword.
        keyword: String,
        /// Decoder diagnostic.
        message: String,
    },
}

/// Recognized numeric metadata fields, independent of serialized spellings.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetadataField {
    Scale,
    CfgRescale,
    VibeStrength,
    VibeInformationExtracted,
}

/// Character conditioning recovered without losing the source list's positions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterMetadata {
    pub source_index: usize,
    pub prompt: Option<String>,
    pub negative_prompt: Option<String>,
}

/// Parsed metadata recovered from one image metadata carrier.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ImageMetadata {
    /// Carrier selected by PNG text, Exif, then stealth precedence.
    pub origin: Option<MetadataOrigin>,
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
    /// V4 character prompts paired by source index, including empty entries.
    pub characters: Vec<CharacterMetadata>,
    /// Recovered vibe metadata entries.
    pub vibe_entries: Vec<VibeMetadataEntry>,
    /// Selected carrier entries grouped by key; Exif and stealth JSON use the same keys.
    pub text_chunks: BTreeMap<String, Vec<String>>,
    /// Parsed JSON payload from the `Comment` chunk when available.
    pub raw_comment_json: Option<Value>,
    /// Recoverable interpretation issues; successfully decoded text chunks remain available.
    pub warnings: Vec<MetadataWarning>,
}

impl std::fmt::Debug for ImageMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageMetadata")
            .field("has_generation_data", &self.has_generation_data())
            .field("origin", &self.origin)
            .field("characters", &self.characters.len())
            .field("vibe_entries", &self.vibe_entries.len())
            .field("text_chunk_keys", &self.text_chunks.len())
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl ImageMetadata {
    /// Returns whether no metadata carrier or interpretation diagnostics were recovered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.origin.is_none()
            && self.text_chunks.is_empty()
            && self.warnings.is_empty()
            && !self.has_generation_data()
            && self.vibe_entries.is_empty()
    }

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
            || !self.characters.is_empty()
    }
}

/// Best-effort metadata inspection state for one generated image.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum GeneratedImageMetadata {
    /// Image metadata was parsed successfully.
    Parsed(Box<ImageMetadata>),
    /// The image is valid but contains no supported metadata carrier.
    NotPresent,
    /// The image format is not supported by the metadata reader.
    UnsupportedFormat,
    /// The payload looked like a supported image but failed media validation.
    Invalid {
        /// Human-readable parser diagnostic.
        message: String,
    },
}

impl GeneratedImageMetadata {
    /// Returns parsed image metadata when available.
    #[must_use]
    pub fn parsed(&self) -> Option<&ImageMetadata> {
        match self {
            Self::Parsed(metadata) => Some(metadata.as_ref()),
            Self::NotPresent | Self::UnsupportedFormat | Self::Invalid { .. } => None,
        }
    }

    /// Returns the per-image seed recovered from embedded metadata.
    #[must_use]
    pub fn seed(&self) -> Option<i64> { self.parsed().and_then(|metadata| metadata.seed) }
}
