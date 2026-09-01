use std::{error::Error as StdError, fmt, time::Duration};

use thiserror::Error;

/// Unified error type returned by this crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// Local tokenizer loading or encoding failed.
    #[cfg(feature = "tokenizer")]
    #[error(transparent)]
    Tokenizer(#[from] crate::TokenizerError),
    /// The request failed local validation.
    #[error(transparent)]
    InvalidRequest(#[from] InvalidRequest),
    /// The `NovelAI` API returned a non-success status.
    #[error(transparent)]
    Api(#[from] ApiError),
    /// A transport-level operation failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// A response or stream payload could not be decoded.
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// PNG metadata parsing or extraction failed.
    #[error(transparent)]
    Metadata(#[from] MetadataError),
}

/// Strongly typed local request validation failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvalidRequest {
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
        source: base64::DecodeError,
    },
    /// A decoded payload was not a supported image.
    #[error("{field} must be a decodable image payload")]
    UndecodableImage {
        field: String,
        #[source]
        source: image::ImageError,
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
    /// A local image preprocessing step failed while encoding PNG bytes.
    #[error("{field} failed to encode as PNG")]
    ImageEncodingFailed {
        field: String,
        #[source]
        source: image::ImageError,
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

/// Non-success response returned by the `NovelAI` API.
#[derive(Debug)]
#[non_exhaustive]
pub struct ApiError {
    /// Category derived from the HTTP status code.
    pub kind: ApiErrorKind,
    /// HTTP status code returned by the API.
    pub status: u16,
    /// Full endpoint URL used for the request.
    pub endpoint: String,
    /// Parsed reason extracted from known API error payload shapes.
    pub server_reason: Option<ApiErrorReason>,
    /// Raw response body text when present.
    pub raw_body: Option<String>,
}

impl ApiError {
    /// Constructs an API failure; optional response details may be assigned through public fields.
    #[must_use]
    pub fn new(kind: ApiErrorKind, status: u16, endpoint: impl Into<String>) -> Self {
        Self {
            kind,
            status,
            endpoint: endpoint.into(),
            server_reason: None,
            raw_body: None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.server_reason {
            Some(reason) => write!(
                f,
                "{}: {} from {} (HTTP {})",
                self.kind, reason, self.endpoint, self.status
            ),
            None => write!(
                f,
                "{} from {} (HTTP {})",
                self.kind, self.endpoint, self.status
            ),
        }
    }
}

impl StdError for ApiError {}

/// API error category derived from HTTP status and headers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorKind {
    /// HTTP 400.
    InvalidRequest,
    /// HTTP 401.
    AuthenticationFailed,
    /// HTTP 402.
    InsufficientCredit,
    /// HTTP 409.
    RequestConflict,
    /// HTTP 429 with optional parsed retry delay.
    RateLimited { retry_after: Option<Duration> },
    /// HTTP 5xx.
    ServerError,
    /// Any status without a specific category.
    UnexpectedStatus,
}

impl fmt::Display for ApiErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest => f.write_str("API rejected the request"),
            Self::AuthenticationFailed => f.write_str("API authentication failed"),
            Self::InsufficientCredit => {
                f.write_str("API reported insufficient credit or subscription access")
            }
            Self::RequestConflict => f.write_str("API reported a request conflict"),
            Self::RateLimited { retry_after } => {
                if let Some(delay) = retry_after {
                    write!(f, "API rate limited the request; retry after {delay:?}")
                } else {
                    f.write_str("API rate limited the request")
                }
            }
            Self::ServerError => f.write_str("API returned a server error"),
            Self::UnexpectedStatus => f.write_str("API returned an unexpected status"),
        }
    }
}

/// Parsed reason extracted from a known API error body field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorReason {
    /// Top-level `message`.
    Message(String),
    /// Top-level `detail`.
    Detail(String),
    /// Nested `error.message`.
    ErrorMessage(String),
}

impl ApiErrorReason {
    /// Returns the extracted reason text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Message(value) | Self::Detail(value) | Self::ErrorMessage(value) => value,
        }
    }
}

impl fmt::Display for ApiErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Transport operation failure.
#[derive(Debug, Error)]
#[error("{operation} failed")]
#[non_exhaustive]
pub struct TransportError {
    /// Transport operation being performed.
    pub operation: TransportOperation,
    /// Full endpoint URL when the failure happened for a specific request.
    pub endpoint: Option<String>,
    /// Original source error.
    #[source]
    pub source: Box<dyn StdError + Send + Sync + 'static>,
}

impl TransportError {
    /// Constructs a transport failure while retaining the original error chain.
    #[must_use]
    pub fn new(
        operation: TransportOperation,
        endpoint: Option<String>,
        source: Box<dyn StdError + Send + Sync + 'static>,
    ) -> Self {
        Self {
            operation,
            endpoint,
            source,
        }
    }
}

/// Transport operation labels used by [`TransportError`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransportOperation {
    /// Constructing an HTTP client.
    BuildClient,
    /// Constructing an HTTP header.
    BuildHeader,
    /// Sending a request.
    SendRequest,
    /// Reading response bytes.
    ReadResponseBytes,
    /// Parsing an SSE stream.
    ParseSse,
}

impl fmt::Display for TransportOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let operation = match self {
            Self::BuildClient => "build HTTP client",
            Self::BuildHeader => "build HTTP header",
            Self::SendRequest => "send HTTP request",
            Self::ReadResponseBytes => "read response bytes",
            Self::ParseSse => "parse SSE stream",
        };
        f.write_str(operation)
    }
}

/// Decode failure for response and stream payloads.
#[derive(Debug, Error)]
#[error("decode {target} failed")]
#[non_exhaustive]
pub struct DecodeError {
    /// Payload target being decoded.
    pub target: DecodeTarget,
    /// Original source error.
    #[source]
    pub source: Box<dyn StdError + Send + Sync + 'static>,
}

impl DecodeError {
    /// Constructs a decode failure while retaining the original error chain.
    #[must_use]
    pub fn new(target: DecodeTarget, source: Box<dyn StdError + Send + Sync + 'static>) -> Self {
        Self { target, source }
    }
}

/// Decode target labels used by [`DecodeError`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeTarget {
    /// HTTP JSON request body.
    JsonRequest,
    /// HTTP JSON response body.
    JsonResponse,
    /// SSE image stream chunk.
    StreamChunk,
}

impl fmt::Display for DecodeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let target = match self {
            Self::JsonRequest => "JSON request",
            Self::JsonResponse => "JSON response",
            Self::StreamChunk => "stream chunk",
        };
        f.write_str(target)
    }
}

/// Metadata helper failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MetadataError {
    /// Raw bytes were not a valid PNG payload.
    #[error("{field} must be valid PNG payload")]
    InvalidPngPayload {
        field: String,
        #[source]
        source: png::DecodingError,
    },
}
