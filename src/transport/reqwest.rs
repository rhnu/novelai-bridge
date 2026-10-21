use std::time::Duration;

use async_trait::async_trait;
#[cfg(feature = "reqwest-streaming")]
use eventsource_stream::Eventsource;
#[cfg(feature = "reqwest-streaming")]
use futures_util::StreamExt;
use secrecy::ExposeSecret;

use super::{
    ResponseLimits, Transport, TransportHeader, TransportMethod, TransportRequest,
    TransportResponse, limits::BodyBuffer,
};
#[cfg(feature = "reqwest-streaming")]
use super::{StreamingTransport, TransportSseEvent, TransportSseResponse};
use crate::{TransportError, TransportOperation};

mod credentials;
pub use credentials::ApiKeySource;

pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Configuration specific to the reqwest HTTP backend.
#[derive(Clone, Debug)]
pub struct ReqwestOptions {
    pub api_key_source: ApiKeySource,
    pub timeout_ms: u64,
    pub response_limits: ResponseLimits,
}

impl Default for ReqwestOptions {
    fn default() -> Self {
        Self {
            api_key_source: ApiKeySource::Environment,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            response_limits: ResponseLimits::default(),
        }
    }
}

/// Default HTTP backend with optional SSE framing.
pub struct ReqwestTransport {
    client: reqwest::Client,
    response_limits: ResponseLimits,
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
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(options.timeout_ms))
            .build()
            .map_err(|source| {
                TransportError::new(TransportOperation::BuildClient, None, Box::new(source))
            })?;
        Ok(Self {
            client,
            response_limits: options.response_limits,
        })
    }

    /// Uses an already configured HTTP client, including its authentication and timeout.
    #[must_use]
    pub fn from_client(client: reqwest::Client) -> Self {
        Self {
            client,
            response_limits: ResponseLimits::default(),
        }
    }

    /// Replaces response budgets independently of authentication and timeout settings.
    #[must_use]
    pub const fn with_response_limits(mut self, limits: ResponseLimits) -> Self {
        self.response_limits = limits;
        self
    }

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
        if let Some(content_type) = request.content_type {
            builder = builder.header(reqwest::header::CONTENT_TYPE, content_type);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
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
        buffer_response(response, endpoint, self.response_limits).await
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
            return buffer_response(response, endpoint, self.response_limits)
                .await
                .map(TransportSseResponse::Http);
        }
        let mut chunks = response.bytes_stream();
        let mut budget = super::limits::SseFrameBudget::new(self.response_limits.sse_event_bytes);
        // Check each raw chunk before eventsource-stream is allowed to append it to its buffers.
        let bounded = async_stream::stream! {
            while let Some(chunk) = chunks.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(error) => { yield Err(SseInputError::Request(error)); return; },
                };
                if let Err(error) = budget.check(&chunk) {
                    yield Err(SseInputError::Limit(error));
                    return;
                }
                yield Ok(chunk);
            }
        };
        let mut events = Box::pin(bounded).eventsource();
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
    mut response: reqwest::Response,
    endpoint: String,
    limits: ResponseLimits,
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
    let successful = response.status().is_success();
    let limit = if successful {
        limits.success_body_bytes
    } else {
        limits.error_body_bytes
    };
    let mut body = BodyBuffer::new(limit, !successful);
    let mut body_truncated = false;
    while let Some(chunk) = response.chunk().await.map_err(|source| {
        TransportError::new(
            TransportOperation::ReadResponseBytes,
            Some(endpoint.clone()),
            Box::new(source),
        )
    })? {
        if !body.append(&chunk).map_err(|source| {
            TransportError::new(
                TransportOperation::ReadResponseBytes,
                Some(endpoint.clone()),
                Box::new(source),
            )
        })? {
            body_truncated = true;
            break;
        }
    }
    Ok(TransportResponse {
        status,
        headers,
        body: body.into_bytes(),
        body_truncated,
    })
}

#[cfg(feature = "reqwest-streaming")]
#[derive(Debug, thiserror::Error)]
enum SseInputError {
    #[error(transparent)]
    Request(reqwest::Error),
    #[error(transparent)]
    Limit(super::ResponseLimitExceeded),
}
