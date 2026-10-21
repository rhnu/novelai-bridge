use async_trait::async_trait;

use crate::TransportError;

mod limits;
pub use limits::{ResponseLimitExceeded, ResponseLimits};

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

/// Complete ordinary HTTP response, including non-success status bodies.
#[derive(Clone, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: Vec<u8>,
    /// Whether an error response body was cut short at its configured limit.
    pub body_truncated: bool,
}

impl std::fmt::Debug for TransportResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TransportResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .field("body_truncated", &self.body_truncated)
            .finish()
    }
}

#[cfg(feature = "streaming")]
pub use crate::generation::TransportSseEvent;

#[cfg(feature = "streaming")]
pub type TransportSseStream = std::pin::Pin<
    Box<dyn futures_core::Stream<Item = Result<TransportSseEvent, TransportError>> + Send>,
>;

/// An opened successful event stream, or an ordinary HTTP response.
#[cfg(feature = "streaming")]
pub enum TransportSseResponse {
    Events(TransportSseStream),
    Http(TransportResponse),
}

/// Ordinary HTTP capability. Serialization and API semantics remain above it.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn execute(&self, request: TransportRequest)
    -> Result<TransportResponse, TransportError>;
}

/// Optional SSE framing capability of the same backend.
#[cfg(feature = "streaming")]
#[async_trait]
pub trait StreamingTransport: Transport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError>;
}

#[cfg(test)]
mod tests;
