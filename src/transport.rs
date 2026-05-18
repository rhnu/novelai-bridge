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

use crate::{BridgeError, BridgeErrorCode};

/// Raw SSE line stream returned by a transport implementation.
pub type TransportSseStream = Pin<Box<dyn Stream<Item = Result<String, BridgeError>> + Send>>;

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
        let auth_header = reqwest::header::HeaderValue::from_str(&auth_value)
            .map_err(|_| BridgeError::authentication_failed("Invalid API key header value"))?;
        headers.insert(reqwest::header::AUTHORIZATION, auth_header);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_millis(timeout_ms))
            .build()
            .map_err(|err| {
                BridgeError::network_error(format!("Failed to initialize HTTP client: {err}"))
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
            .map_err(|err| BridgeError::network_error(format!("Network error: {err}")))?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status_error(status, &headers, body_text, endpoint));
        }

        response.json::<TRes>().await.map_err(|err| {
            BridgeError::unknown_error(format!("Failed to deserialize JSON response: {err}"))
        })
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
            .map_err(|err| BridgeError::network_error(format!("Network error: {err}")))?;

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
            .map_err(|err| {
                BridgeError::unknown_error(format!("Failed to read response bytes: {err}"))
            })
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
            .map_err(|err| BridgeError::network_error(format!("Network error: {err}")))?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_status_error(status, &headers, body_text, endpoint));
        }

        let mut event_stream = response.bytes_stream().eventsource();
        let output = try_stream! {
            while let Some(item) = event_stream.next().await {
                match item {
                    Ok(event) => {
                        if !event.data.is_empty() {
                            yield format!("data: {}", event.data);
                        }
                    }
                    Err(err) => {
                        Err(BridgeError::network_error(format!("SSE parse error: {err}")))?;
                    }
                }
            }
        };

        Ok(Box::pin(output))
    }
}

fn map_status_error(
    status: reqwest::StatusCode,
    headers: &HeaderMap,
    body_text: String,
    endpoint: &str,
) -> BridgeError {
    let trimmed = body_text.trim();
    let message = if trimmed.is_empty() {
        format!("Request failed: {}", status.as_u16())
    } else {
        extract_error_message(trimmed).unwrap_or_else(|| trimmed.to_owned())
    };

    match status.as_u16() {
        400 => BridgeError::invalid_request(message),
        401 => BridgeError::authentication_failed(if message.contains("401") {
            "Invalid API key".to_owned()
        } else {
            message
        }),
        402 => BridgeError::insufficient_credit(if message.contains("402") {
            "Insufficient credits or subscription required".to_owned()
        } else {
            message
        }),
        409 => BridgeError::request_conflict(message),
        429 => BridgeError::rate_limited(
            message,
            parse_retry_after_ms(
                headers
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok()),
                SystemTime::now(),
            ),
        ),
        500..=599 => BridgeError::server_error(message),
        _ => BridgeError::new(
            BridgeErrorCode::UnknownError,
            format!("{message} (endpoint: {endpoint})"),
        ),
    }
}

fn extract_error_message(raw: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    if let Some(message) = parsed.get("message").and_then(serde_json::Value::as_str) {
        return Some(message.to_owned());
    }
    if let Some(message) = parsed.get("detail").and_then(serde_json::Value::as_str) {
        return Some(message.to_owned());
    }
    parsed
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn parse_retry_after_ms(raw: Option<&str>, now: SystemTime) -> Option<u64> {
    let value = raw?;

    if let Ok(seconds) = value.trim().parse::<i64>() {
        let seconds = seconds.max(0) as u64;
        return Some(seconds.saturating_mul(1000));
    }

    if let Ok(date) = httpdate::parse_http_date(value.trim()) {
        return Some(
            date.duration_since(now)
                .unwrap_or_else(|_| Duration::from_millis(0))
                .as_millis() as u64,
        );
    }

    Some(0)
}

#[cfg(test)]
mod tests;
