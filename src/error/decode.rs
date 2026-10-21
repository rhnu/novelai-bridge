use std::{error::Error as StdError, fmt};

use thiserror::Error;

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
    /// Raw HTTP response body before operation-specific decoding.
    HttpResponseBody,
    /// HTTP JSON response body.
    JsonResponse,
    /// SSE image stream chunk.
    StreamChunk,
    /// Image-generation or director response body.
    ImageResponse,
}

impl fmt::Display for DecodeTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let target = match self {
            Self::HttpResponseBody => "HTTP response body",
            Self::JsonResponse => "JSON response",
            Self::StreamChunk => "stream chunk",
            Self::ImageResponse => "image response",
        };
        f.write_str(target)
    }
}
