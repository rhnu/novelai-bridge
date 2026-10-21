use async_trait::async_trait;

use crate::TransportError;

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportRequest {
    pub method: TransportMethod,
    pub endpoint: String,
    pub accept: Option<String>,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportHeader {
    pub name: String,
    pub value: Vec<u8>,
}

/// Complete ordinary HTTP response, including non-success status bodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: Vec<u8>,
}

#[cfg(feature = "streaming")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportSseEvent {
    pub data: String,
}

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
