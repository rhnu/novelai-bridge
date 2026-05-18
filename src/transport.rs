use std::{
    pin::Pin,
    time::{Duration, SystemTime},
};

use async_stream::try_stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_core::Stream;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use secrecy::{ExposeSecret, SecretString};
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DecodeError, DecodeTarget,
    ImageStreamChunk, TransportError, TransportOperation,
};

/// Parsed SSE chunk stream returned by a transport implementation.
pub type TransportSseStream =
    Pin<Box<dyn Stream<Item = Result<ImageStreamChunk, BridgeError>> + Send>>;

/// Abstraction over the HTTP transport used by [`crate::Client`].
///
/// Each method receives a complete endpoint URL resolved by the client.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sends a GET request to `endpoint` and deserializes a JSON response.
    async fn get_json<TRes: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
    ) -> Result<TRes, BridgeError>;

    /// Sends a JSON POST request to `endpoint` and returns raw response bytes.
    async fn post_bytes<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<Vec<u8>, BridgeError>;

    /// Sends a JSON POST request to `endpoint` and returns an SSE stream.
    async fn post_sse<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<TransportSseStream, BridgeError>;
}

/// Default [`Transport`] implementation backed by `reqwest + rustls`.
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Creates a transport with a resolved API key.
    pub fn new(api_key: SecretString, timeout_ms: u64) -> Result<Self, BridgeError> {
        let mut headers = HeaderMap::new();
        let auth_value = format!("Bearer {}", api_key.expose_secret());
        let auth_header =
            reqwest::header::HeaderValue::from_str(&auth_value).map_err(|source| {
                TransportError {
                    operation: TransportOperation::BuildHeader,
                    endpoint: None,
                    source: Box::new(source),
                }
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
            .map_err(|source| TransportError {
                operation: TransportOperation::BuildClient,
                endpoint: None,
                source: Box::new(source),
            })?;

        Ok(Self { client })
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn get_json<TRes: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
    ) -> Result<TRes, BridgeError> {
        let response = self
            .client
            .get(endpoint)
            .send()
            .await
            .map_err(|source| TransportError {
                operation: TransportOperation::SendRequest,
                endpoint: Some(endpoint.to_owned()),
                source: Box::new(source),
            })?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status_error(status, &headers, body_text, endpoint));
        }

        response
            .json::<TRes>()
            .await
            .map_err(|source| DecodeError {
                target: DecodeTarget::JsonResponse,
                source: Box::new(source),
            })
            .map_err(Into::into)
    }

    async fn post_bytes<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<Vec<u8>, BridgeError> {
        let response = self
            .client
            .post(endpoint)
            .json(body)
            .send()
            .await
            .map_err(|source| TransportError {
                operation: TransportOperation::SendRequest,
                endpoint: Some(endpoint.to_owned()),
                source: Box::new(source),
            })?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status_error(status, &headers, body_text, endpoint));
        }

        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|source| TransportError {
                operation: TransportOperation::ReadResponseBytes,
                endpoint: Some(endpoint.to_owned()),
                source: Box::new(source),
            })
            .map_err(Into::into)
    }

    async fn post_sse<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<TransportSseStream, BridgeError> {
        let response = self
            .client
            .post(endpoint)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .json(body)
            .send()
            .await
            .map_err(|source| TransportError {
                operation: TransportOperation::SendRequest,
                endpoint: Some(endpoint.to_owned()),
                source: Box::new(source),
            })?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status_error(status, &headers, body_text, endpoint));
        }

        let mut event_stream = response.bytes_stream().eventsource();
        let endpoint = endpoint.to_owned();
        let output = try_stream! {
            while let Some(item) = event_stream.next().await {
                match item {
                    Ok(event) => {
                        if !event.data.is_empty() {
                            yield parse_stream_chunk(&event.data)?;
                        }
                    }
                    Err(err) => {
                        Err(TransportError {
                            operation: TransportOperation::ParseSse,
                            endpoint: Some(endpoint.clone()),
                            source: Box::new(err),
                        })?;
                    }
                }
            }
        };

        Ok(Box::pin(output))
    }
}

fn parse_stream_chunk(data: &str) -> Result<ImageStreamChunk, BridgeError> {
    serde_json::from_str(data)
        .map_err(|source| DecodeError {
            target: DecodeTarget::StreamChunk,
            source: Box::new(source),
        })
        .map_err(Into::into)
}

fn map_status_error(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    body_text: String,
    endpoint: &str,
) -> BridgeError {
    let trimmed = body_text.trim();
    let server_reason = if trimmed.is_empty() {
        None
    } else {
        extract_error_reason(trimmed)
    };
    let raw_body = (!trimmed.is_empty()).then(|| trimmed.to_owned());

    ApiError {
        kind: match status.as_u16() {
            400 => ApiErrorKind::InvalidRequest,
            401 => ApiErrorKind::AuthenticationFailed,
            402 => ApiErrorKind::InsufficientCredit,
            409 => ApiErrorKind::RequestConflict,
            429 => ApiErrorKind::RateLimited {
                retry_after: parse_retry_after(
                    headers
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                    SystemTime::now(),
                ),
            },
            500..=599 => ApiErrorKind::ServerError,
            _ => ApiErrorKind::UnexpectedStatus,
        },
        status: status.as_u16(),
        endpoint: endpoint.to_owned(),
        server_reason,
        raw_body,
    }
    .into()
}

fn extract_error_reason(raw: &str) -> Option<ApiErrorReason> {
    let parsed = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    if let Some(message) = parsed.get("message").and_then(serde_json::Value::as_str) {
        return Some(ApiErrorReason::Message(message.to_owned()));
    }
    if let Some(detail) = parsed.get("detail").and_then(serde_json::Value::as_str) {
        return Some(ApiErrorReason::Detail(detail.to_owned()));
    }
    parsed
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(|message| ApiErrorReason::ErrorMessage(message.to_owned()))
}

fn parse_retry_after(raw: Option<&str>, now: SystemTime) -> Option<Duration> {
    let value = raw?;

    if let Ok(seconds) = value.trim().parse::<i64>() {
        if seconds < 0 {
            return None;
        }
        return Some(Duration::from_secs(seconds as u64));
    }

    if let Ok(date) = httpdate::parse_http_date(value.trim()) {
        return Some(date.duration_since(now).unwrap_or(Duration::ZERO));
    }

    None
}

#[cfg(test)]
mod tests;
