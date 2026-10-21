use async_trait::async_trait;

use crate::TransportError;
pub use crate::protocol::limits::{ResponseLimitExceeded, ResponseLimits};

#[cfg(feature = "reqwest-transport")]
mod reqwest;
#[cfg(feature = "reqwest-transport")]
pub use reqwest::{ApiKeySource, DEFAULT_TIMEOUT_MS, ReqwestOptions, ReqwestTransport};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransportMethod {
    Get,
    Post,
}

/// Encoded HTTP request; authentication is owned by the transport.
#[derive(Clone, PartialEq, Eq)]
pub struct TransportRequest {
    pub method: TransportMethod,
    pub endpoint: String,
    pub accept: Option<String>,
    pub content_type: Option<String>,
    pub body: Option<Vec<u8>>,
}

impl std::fmt::Debug for TransportRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransportRequest")
            .field("method", &self.method)
            .field("endpoint", &self.endpoint)
            .field("accept", &self.accept)
            .field("content_type", &self.content_type)
            .field("body_bytes", &self.body.as_ref().map(Vec::len))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportHeader {
    pub name: String,
    pub value: Vec<u8>,
}

/// Raw HTTP body chunks. Framing and response budgets are owned by the protocol layer.
pub type TransportByteStream =
    std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Vec<u8>, TransportError>> + Send>>;

/// HTTP response headers and its unconsumed byte stream, including non-success statuses.
pub struct TransportResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: TransportByteStream,
}

impl TransportResponse {
    /// Wraps an already buffered body for a custom transport.
    #[must_use]
    pub fn from_bytes(status: u16, headers: Vec<TransportHeader>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body: Box::pin(futures_util::stream::iter([Ok(body)])),
        }
    }
}

impl std::fmt::Debug for TransportResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransportResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .finish_non_exhaustive()
    }
}

/// HTTP capability. Serialization, body limits and framing remain above it.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn execute(&self, request: TransportRequest)
    -> Result<TransportResponse, TransportError>;
}

#[cfg(test)]
mod tests;
