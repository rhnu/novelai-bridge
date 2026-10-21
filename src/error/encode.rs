use std::{error::Error as StdError, fmt};

use thiserror::Error;

/// Failure to encode a request payload.
#[derive(Debug, Error)]
#[error("encode {target} failed")]
#[non_exhaustive]
pub struct EncodeError {
    pub target: EncodeTarget,
    #[source]
    pub source: Box<dyn StdError + Send + Sync + 'static>,
}

impl EncodeError {
    #[must_use]
    pub fn new(target: EncodeTarget, source: Box<dyn StdError + Send + Sync + 'static>) -> Self {
        Self { target, source }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeTarget {
    /// HTTP JSON request body.
    JsonRequest,
}

impl fmt::Display for EncodeTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::JsonRequest => formatter.write_str("JSON request"),
        }
    }
}
