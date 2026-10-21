use std::time::{Duration, SystemTime};

use crate::{ApiError, ApiErrorKind, ApiErrorReason, TransportHeader, TransportResponse};

pub(super) fn checked_body(response: TransportResponse, endpoint: &str) -> Result<Vec<u8>, ApiError> {
    if (200..300).contains(&response.status) {
        Ok(response.body)
    } else {
        Err(map_status_error(response.status, &response.headers, &response.body, endpoint))
    }
}

pub fn map_status_error(
    status: u16,
    headers: &[TransportHeader],
    body: &[u8],
    endpoint: &str,
) -> ApiError {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    let server_reason = (!trimmed.is_empty())
        .then(|| extract_error_reason(trimmed))
        .flatten();
    ApiError {
        kind: match status {
            400 => ApiErrorKind::InvalidRequest,
            401 => ApiErrorKind::AuthenticationFailed,
            402 => ApiErrorKind::InsufficientCredit,
            409 => ApiErrorKind::RequestConflict,
            429 => ApiErrorKind::RateLimited {
                retry_after: parse_retry_after(header(headers, "retry-after"), SystemTime::now()),
            },
            500..=599 => ApiErrorKind::ServerError,
            _ => ApiErrorKind::UnexpectedStatus,
        },
        status,
        endpoint: endpoint.to_owned(),
        server_reason,
        raw_body: (!trimmed.is_empty()).then(|| trimmed.to_owned()),
    }

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
