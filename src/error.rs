use thiserror::Error;

mod api;
mod decode;
mod encode;
mod metadata;
mod preparation;
mod request;
mod transport;

pub use api::{ApiError, ApiErrorKind, ApiErrorReason};
pub use decode::{DecodeError, DecodeTarget};
pub use encode::{EncodeError, EncodeTarget};
pub use metadata::MetadataError;
pub use preparation::PreparationError;
pub use request::{DataUrlError, InvalidRequest};
pub use transport::{TransportError, TransportOperation};

/// Unified error type returned by this crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum BridgeError {
    /// Valid input could not be prepared for execution.
    #[error(transparent)]
    Preparation(#[from] PreparationError),
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
    /// A request payload could not be encoded.
    #[error(transparent)]
    Encode(#[from] EncodeError),
    /// A response or stream payload could not be decoded.
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// PNG metadata parsing or extraction failed.
    #[error(transparent)]
    Metadata(#[from] MetadataError),
}

#[cfg(test)]
mod tests;
