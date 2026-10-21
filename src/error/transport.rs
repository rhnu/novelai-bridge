use std::{error::Error as StdError, fmt};

use thiserror::Error;

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
    /// Reading or validating backend credentials.
    ResolveCredentials,
    /// Constructing an HTTP client.
    BuildClient,
    /// Constructing an HTTP header.
    BuildHeader,
    /// Sending a request.
    SendRequest,
    /// Reading response bytes.
    ReadResponseBytes,
}

impl fmt::Display for TransportOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let operation = match self {
            Self::ResolveCredentials => "resolve credentials",
            Self::BuildClient => "build HTTP client",
            Self::BuildHeader => "build HTTP header",
            Self::SendRequest => "send HTTP request",
            Self::ReadResponseBytes => "read response bytes",
        };
        f.write_str(operation)
    }
}
