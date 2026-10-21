use std::error::Error as StdError;

use thiserror::Error;

use crate::{ImageSize, RequestField};

/// A local execution failure after validating request input.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PreparationError {
    #[error("failed to generate image-request randomness")]
    RequestRandomnessFailed {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("failed to generate a request seed")]
    SeedGenerationFailed {
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("{field} failed to encode as PNG")]
    ImageEncodingFailed {
        field: RequestField,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
    #[error("{field} image size {actual:?} differs from required size {expected:?}")]
    ImageSizeMismatch {
        field: RequestField,
        expected: ImageSize,
        actual: ImageSize,
    },
}
