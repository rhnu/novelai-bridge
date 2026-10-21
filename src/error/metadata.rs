use std::error::Error as StdError;

use thiserror::Error;

/// Metadata helper failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MetadataError {
    /// Raw bytes were not a valid PNG payload.
    #[error("{field} must be valid PNG payload")]
    InvalidPngPayload {
        field: crate::RequestField,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },
}
