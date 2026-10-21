use std::{error::Error as StdError, fmt};

use thiserror::Error;

use crate::RequestField;

/// Strongly typed local request validation failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRequest {
    /// A final conditioning text exceeds the selected model's token limit.
    #[error("{field} uses {used} tokens but the model limit is {limit}")]
    PromptTokenLimitExceeded {
        field: RequestField,
        used: u32,
        limit: u32,
    },
    /// A required field was blank after trimming.
    #[error("{field} cannot be empty")]
    EmptyField { field: RequestField },
    /// A finite numeric value was outside the API-supported range.
    #[error("{field} must be between {min} and {max}")]
    NumericOutOfRange {
        field: RequestField,
        value: f64,
        min: f64,
        max: f64,
    },
    /// An image dimension violated `NovelAI` size constraints.
    #[error("{field} must be in [{min},{max}] and multiple of {multiple_of}")]
    InvalidImageDimension {
        field: RequestField,
        value: u32,
        min: u32,
        max: u32,
        multiple_of: u32,
    },
    /// A numeric value was NaN or infinite.
    #[error("{field} must be finite")]
    NonFiniteNumber { field: RequestField, value: f64 },
    /// A data URL was malformed or unsupported.
    #[error("{field} data URL is invalid: {reason}")]
    InvalidDataUrl {
        field: RequestField,
        reason: DataUrlError,
    },
    /// A base64 payload failed to decode.
    #[error("{field} must be valid base64 payload")]
    InvalidBase64 {
        field: RequestField,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    /// A decoded payload was not a supported image.
    #[error("{field} must be a decodable image payload")]
    UndecodableImage {
        field: RequestField,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    /// A request feature is only supported by a different model family.
    #[error("{feature} is only supported on {required_model}")]
    UnsupportedModelFeature {
        feature: RequestField,
        required_model: String,
    },
    /// Two request fields or feature groups cannot be used together.
    #[error("{left} and {right} cannot be used together")]
    UnsupportedFieldCombination {
        left: RequestField,
        right: RequestField,
    },
    /// A required non-empty prompt was missing for a selected tool or mode.
    #[error("{context} requires non-empty {field}")]
    RequiredFieldForContext {
        context: String,
        field: RequestField,
    },
    /// An image had impossible dimensions after decoding.
    #[error("{field} image size cannot be zero")]
    ZeroImageDimension { field: RequestField },
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
