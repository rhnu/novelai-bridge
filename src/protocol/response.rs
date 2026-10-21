use std::time::{Duration, SystemTime};

use crate::{ApiError, ApiErrorReason, TransportHeader, TransportResponse};

/// A successful, bounded response retained until operation-specific decoding.
pub struct BufferedResponse {
    pub status: u16,
    pub headers: Vec<TransportHeader>,
    pub body: Vec<u8>,
}

impl std::fmt::Debug for BufferedResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BufferedResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

impl BufferedResponse {
    /// Returns the first matching header, using case-insensitive HTTP field names.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_slice())
    }
}

pub async fn checked_body(
    response: TransportResponse,
    endpoint: &str,
    limits: super::limits::ResponseLimits,
) -> Result<BufferedResponse, crate::BridgeError> {
    use futures_util::StreamExt;
    let TransportResponse {
        status,
        headers,
        mut body,
    } = response;
    let successful = (200..300).contains(&status);
    let limit = if successful {
        limits.success_body_bytes
    } else {
        limits.error_body_bytes
    };
    let mut buffer = super::limits::BodyBuffer::new(limit, !successful);
    let mut truncated = false;
    while let Some(chunk) = body.next().await {
        let chunk = chunk?;
        if !buffer.append(&chunk).map_err(|source| {
            crate::DecodeError::new(crate::DecodeTarget::HttpResponseBody, Box::new(source))
        })? {
            truncated = true;
            break;
        }
    }
    let body = buffer.into_bytes();
    if successful {
        Ok(BufferedResponse {
            status,
            headers,
            body,
        })
    } else {
        Err(map_status_error(status, &headers, &body, endpoint, truncated).into())
    }
}

pub fn map_status_error(
    status: u16,
    headers: &[TransportHeader],
    body: &[u8],
    endpoint: &str,
    body_truncated: bool,
) -> ApiError {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    let server_reason = (!trimmed.is_empty())
        .then(|| extract_error_reason(trimmed))
        .flatten();
    ApiError::from_response_parts(
        status,
        endpoint,
        parse_retry_after(header(headers, "retry-after"), SystemTime::now()),
        server_reason,
        (!trimmed.is_empty()).then(|| trimmed.to_owned()),
        body_truncated,
    )
}

fn header<'a>(headers: &'a [TransportHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .and_then(|header| std::str::from_utf8(&header.value).ok())
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
        .get("error")?
        .get("message")?
        .as_str()
        .map(|message| ApiErrorReason::ErrorMessage(message.to_owned()))
}

fn parse_retry_after(raw: Option<&str>, now: SystemTime) -> Option<Duration> {
    let value = raw?;
    if let Ok(seconds) = value.trim().parse::<i64>() {
        return (seconds >= 0).then(|| Duration::from_secs(seconds.cast_unsigned()));
    }
    httpdate::parse_http_date(value.trim())
        .ok()
        .map(|date| date.duration_since(now).unwrap_or(Duration::ZERO))
}

#[cfg(test)]
mod tests;
