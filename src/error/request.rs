use std::{error::Error as StdError, fmt};

use thiserror::Error;

/// Strongly typed local request validation failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRequest {
    /// The operating system could not provide a generation seed.
    #[error("failed to generate a request seed")]
    SeedGenerationFailed {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    /// A final conditioning text exceeds the selected model's token limit.
    #[error("{field} uses {used} tokens but the model limit is {limit}")]
    PromptTokenLimitExceeded {
        field: String,
        used: u32,
        limit: u32,
    },
    /// A required field was blank after trimming.
    #[error("{field} cannot be empty")]
    EmptyField { field: String },
    /// A required configuration value was not present.
    #[error("{name} is not configured")]
    MissingConfiguration { name: String },
    /// A finite numeric value was outside the API-supported range.
    #[error("{field} must be between {min} and {max}")]
    NumericOutOfRange {
        field: String,
        value: f64,
        min: f64,
        max: f64,
    },
    /// An image dimension violated `NovelAI` size constraints.
    #[error("{field} must be in [{min},{max}] and multiple of {multiple_of}")]
    InvalidImageDimension {
        field: String,
        value: u32,
        min: u32,
        max: u32,
        multiple_of: u32,
    },
    /// A numeric value was NaN or infinite.
    #[error("{field} must be finite")]
    NonFiniteNumber { field: String, value: f64 },
    /// A data URL was malformed or unsupported.
    #[error("{field} data URL is invalid: {reason}")]
    InvalidDataUrl { field: String, reason: DataUrlError },
    /// A base64 payload failed to decode.
    #[error("{field} must be valid base64 payload")]
    InvalidBase64 {
        field: String,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    /// A decoded payload was not a supported image.
    #[error("{field} must be a decodable image payload")]
    UndecodableImage {
        field: String,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    /// A request feature is only supported by a different model family.
    #[error("{feature} is only supported on {required_model}")]
    UnsupportedModelFeature {
        feature: String,
        required_model: String,
    },
    /// Two request fields or feature groups cannot be used together.
    #[error("{left} and {right} cannot be used together")]
    UnsupportedFieldCombination { left: String, right: String },
    /// A field cannot be supplied for the selected tool or mode.
    #[error("{field} is not supported for {context}")]
    UnsupportedFieldForContext { field: String, context: String },
    /// A required non-empty prompt was missing for a selected tool or mode.
    #[error("{context} requires non-empty {field}")]
    RequiredFieldForContext { context: String, field: String },
    /// An image had impossible dimensions after decoding.
    #[error("{field} image size cannot be zero")]
    ZeroImageDimension { field: String },
    /// An injected image preprocessor returned the wrong canvas.
    #[cfg(feature = "image-processing")]
    #[error("{field} image size {actual:?} differs from the required size {expected:?}")]
    PreparedImageSizeMismatch {
        field: String,
        expected: crate::ImageSize,
        actual: crate::ImageSize,
    },
    /// A local image preprocessing step failed while encoding PNG bytes.
    #[error("{field} failed to encode as PNG")]
    ImageEncodingFailed {
        field: String,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
}

/// Specific data URL validation failure.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataUrlError {
    /// The URL did not contain the metadata/payload separator.
    MissingPayloadSeparator,
    /// The URL media type was not an image.
    UnsupportedMediaType,
    /// The URL payload was not marked as base64 encoded.
    MissingBase64Marker,
}

impl fmt::Display for DataUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingPayloadSeparator => "missing comma separator",
            Self::UnsupportedMediaType => "only image data URLs are supported",
            Self::MissingBase64Marker => "payload must be base64 encoded",
        };
        f.write_str(message)
    }
}
