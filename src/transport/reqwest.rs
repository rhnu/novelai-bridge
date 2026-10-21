use std::time::Duration;

use async_trait::async_trait;
#[cfg(feature = "reqwest-streaming")]
use eventsource_stream::Eventsource;
#[cfg(feature = "reqwest-streaming")]
use futures_util::StreamExt;
use secrecy::ExposeSecret;

#[cfg(feature = "reqwest-streaming")]
use super::{StreamingTransport, TransportSseEvent, TransportSseResponse};
use super::{Transport, TransportHeader, TransportMethod, TransportRequest, TransportResponse};
use crate::{TransportError, TransportOperation};

mod credentials;
pub use credentials::ApiKeySource;

pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Configuration specific to the reqwest HTTP backend.
#[derive(Clone, Debug)]
pub struct ReqwestOptions {
    pub api_key_source: ApiKeySource,
    pub timeout_ms: u64,
}

impl Default for ReqwestOptions {
    fn default() -> Self {
        Self {
            api_key_source: ApiKeySource::Environment,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

/// Default HTTP backend with optional SSE framing.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Constructs an authenticated HTTP backend.
    ///
    /// # Errors
    /// Returns credential, header or HTTP client construction failures.
    pub fn new(options: ReqwestOptions) -> Result<Self, TransportError> {
        let api_key = credentials::resolve(options.api_key_source)?;
        let mut authorization =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", api_key.expose_secret()))
                .map_err(|source| {
                    TransportError::new(TransportOperation::BuildHeader, None, Box::new(source))
                })?;
        authorization.set_sensitive(true);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, authorization);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(options.timeout_ms))
            .build()
            .map_err(|source| {
                TransportError::new(TransportOperation::BuildClient, None, Box::new(source))
            })?;
        Ok(Self { client })
    }

    /// Uses an already configured HTTP client, including its authentication and timeout.
    #[must_use]
    pub const fn from_client(client: reqwest::Client) -> Self { Self { client } }

    async fn send(
        &self,
        request: TransportRequest,
    ) -> Result<(reqwest::Response, String), TransportError> {
        let mut builder = match request.method {
            TransportMethod::Get => self.client.get(&request.endpoint),
            TransportMethod::Post => self.client.post(&request.endpoint),
        };
        if let Some(accept) = request.accept {
            builder = builder.header(reqwest::header::ACCEPT, accept);
        }
        if let Some(body) = request.body {
            builder = builder
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let endpoint = request.endpoint;
        let response = builder.send().await.map_err(|source| {
            TransportError::new(
                TransportOperation::SendRequest,
                Some(endpoint.clone()),
                Box::new(source),
            )
        })?;
        Ok((response, endpoint))
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let (response, endpoint) = self.send(request).await?;
        buffer_response(response, endpoint).await
    }
}

#[cfg(feature = "reqwest-streaming")]
#[async_trait]
impl StreamingTransport for ReqwestTransport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        let (response, endpoint) = self.send(request).await?;
        if !response.status().is_success() || !is_event_stream(response.headers()) {
            return buffer_response(response, endpoint)
                .await
                .map(TransportSseResponse::Http);
        }
        let mut events = response.bytes_stream().eventsource();
        let output = async_stream::try_stream! {
            while let Some(item) = events.next().await {
                let event = item.map_err(|source| TransportError::new(
                    TransportOperation::ParseSse, Some(endpoint.clone()), Box::new(source)))?;
                if !event.data.is_empty() { yield TransportSseEvent { data: event.data }; }
            }
        };
        Ok(TransportSseResponse::Events(Box::pin(output)))
    }
}

#[cfg(feature = "reqwest-streaming")]
fn is_event_stream(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("text/event-stream"))
}

#[cfg(test)]
mod tests;

async fn buffer_response(
    response: reqwest::Response,
    endpoint: String,
) -> Result<TransportResponse, TransportError> {
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| TransportHeader {
            name: name.as_str().to_owned(),
            value: value.as_bytes().to_vec(),
        })
        .collect();
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
