use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use secrecy::ExposeSecret;

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

/// Authenticated HTTP backend returning raw response bytes.
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
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| TransportHeader {
                name: name.as_str().to_owned(),
                value: value.as_bytes().to_vec(),
            })
            .collect();
        let body = response.bytes_stream().map(move |chunk| {
            chunk.map(|bytes| bytes.to_vec()).map_err(|source| {
                TransportError::new(
                    TransportOperation::ReadResponseBytes,
                    Some(endpoint.clone()),
                    Box::new(source),
                )
            })
        });
        Ok(TransportResponse {
            status,
            headers,
            body: Box::pin(body),
        })
    }
}
