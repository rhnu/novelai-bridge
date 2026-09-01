use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_core::Stream;
use futures_util::StreamExt;
use secrecy::{ExposeSecret, SecretString};

use crate::{BridgeError, TransportError, TransportOperation};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransportMethod {
    Get,
    Post,
}

/// Fully encoded HTTP request passed to a transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportRequest {
    pub method: TransportMethod,
    pub endpoint: String,
    pub accept: Option<String>,
    /// Serialized JSON, absent for GET.
    pub body: Option<Vec<u8>>,
}

/// Raw response header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportHeader {
    pub name: String,
    pub value: Vec<u8>,
}

/// Complete ordinary HTTP response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: Vec<u8>,
}

/// Framed SSE event without application-specific fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportSseEvent {
    pub data: String,
}

pub type TransportSseStream =
    Pin<Box<dyn Stream<Item = Result<TransportSseEvent, TransportError>> + Send>>;

pub enum TransportSseBody {
    Events(TransportSseStream),
    Bytes(Vec<u8>),
}

pub struct TransportSseResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: TransportSseBody,
}

/// Ordinary HTTP adapter. JSON and API semantics remain above this boundary.
#[async_trait]
pub trait Transport: Send + Sync {
    async fn execute(&self, request: TransportRequest)
    -> Result<TransportResponse, TransportError>;
}

/// Optional SSE capability, separate from ordinary HTTP transport.
#[async_trait]
pub trait StreamingTransport: Transport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError>;
}

/// Default transport backed by `reqwest + rustls`.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Creates a transport with a resolved API key.
    ///
    /// # Errors
    ///
    /// Returns a transport error when the authorization header or HTTP client cannot be built.
    pub fn new(api_key: SecretString, timeout_ms: u64) -> Result<Self, BridgeError> {
        let mut headers = reqwest::header::HeaderMap::new();
        let auth_value = format!("Bearer {}", api_key.expose_secret());
        drop(api_key);
        let auth_header =
            reqwest::header::HeaderValue::from_str(&auth_value).map_err(|source| {
                TransportError::new(TransportOperation::BuildHeader, None, Box::new(source))
            })?;
        headers.insert(reqwest::header::AUTHORIZATION, auth_header);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|source| {
                TransportError::new(TransportOperation::BuildClient, None, Box::new(source))
            })?;
        Ok(Self { client })
    }

    fn build(&self, request: &TransportRequest) -> reqwest::RequestBuilder {
        let mut builder = match request.method {
            TransportMethod::Get => self.client.get(&request.endpoint),
            TransportMethod::Post => self.client.post(&request.endpoint),
        };
        if let Some(accept) = &request.accept {
            builder = builder.header(reqwest::header::ACCEPT, accept);
        }
        if let Some(body) = &request.body {
            builder = builder.body(body.clone());
        }
        builder
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let endpoint = request.endpoint.clone();
        let response = self.build(&request).send().await.map_err(|source| {
            TransportError::new(
                TransportOperation::SendRequest,
                Some(endpoint.clone()),
                Box::new(source),
            )
        })?;
        let status = response.status().as_u16();
        let headers = collect_headers(response.headers());
        let body = response.bytes().await.map_err(|source| {
            TransportError::new(
                TransportOperation::ReadResponseBytes,
                Some(endpoint),
                Box::new(source),
            )
        })?;
        Ok(TransportResponse {
            status,
            headers,
            body: body.to_vec(),
        })
    }
}

#[async_trait]
impl StreamingTransport for ReqwestTransport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        let endpoint = request.endpoint.clone();
        let response = self.build(&request).send().await.map_err(|source| {
            TransportError::new(
                TransportOperation::SendRequest,
                Some(endpoint.clone()),
                Box::new(source),
            )
        })?;
        let status = response.status().as_u16();
        let headers = collect_headers(response.headers());
        if !(200..300).contains(&status) {
            let body = response.bytes().await.map_err(|source| {
                TransportError::new(
                    TransportOperation::ReadResponseBytes,
                    Some(endpoint),
                    Box::new(source),
                )
            })?;
            return Ok(TransportSseResponse {
                status,
                headers,
                body: TransportSseBody::Bytes(body.to_vec()),
            });
        }
        let mut events = response.bytes_stream().eventsource();
        let output = async_stream::try_stream! {
            while let Some(item) = events.next().await {
                let event = item.map_err(|source| TransportError::new(
                    TransportOperation::ParseSse, Some(endpoint.clone()), Box::new(source)))?;
                if !event.data.is_empty() { yield TransportSseEvent { data: event.data }; }
            }
        };
        Ok(TransportSseResponse {
            status,
            headers,
            body: TransportSseBody::Events(Box::pin(output)),
        })
    }
}

fn collect_headers(headers: &reqwest::header::HeaderMap) -> Vec<TransportHeader> {
    headers
        .iter()
        .map(|(name, value)| TransportHeader {
            name: name.as_str().to_owned(),
            value: value.as_bytes().to_vec(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
