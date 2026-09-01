use std::time::{Duration, SystemTime};

use futures_util::StreamExt;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DecodeError, DecodeTarget,
    ImageStreamChunk, StreamingTransport, Transport, TransportHeader, TransportMethod,
    TransportRequest, TransportResponse, TransportSseBody, TransportSseStream,
};

pub async fn get_json<T: Transport, R: DeserializeOwned>(
    transport: &T,
    endpoint: &str,
) -> Result<R, BridgeError> {
    let response = transport
        .execute(TransportRequest {
            method: TransportMethod::Get,
            endpoint: endpoint.to_owned(),
            accept: Some("application/json".to_owned()),
            body: None,
        })
        .await?;
    let body = checked_body(response, endpoint)?;
    serde_json::from_slice(&body)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)).into())
}

pub async fn post_bytes<T: Transport, B: Serialize + Sync>(
    transport: &T,
    endpoint: &str,
    body: &B,
) -> Result<Vec<u8>, BridgeError> {
    let response = transport
        .execute(json_request(endpoint, body, "application/octet-stream")?)
        .await?;
    checked_body(response, endpoint)
}

pub async fn post_sse<T: StreamingTransport, B: Serialize + Sync>(
    transport: &T,
    endpoint: &str,
    body: &B,
) -> Result<crate::ImageStreamResult, BridgeError> {
    let response = transport
        .execute_sse(json_request(endpoint, body, "text/event-stream")?)
        .await?;
    if !(200..300).contains(&response.status) {
        let body = match response.body {
            TransportSseBody::Bytes(body) => body,
            TransportSseBody::Events(_) => Vec::new(),
        };
        return Err(map_status_error(
            response.status,
            &response.headers,
            &body,
            endpoint,
        ));
    }
    let TransportSseBody::Events(stream) = response.body else {
        return Err(DecodeError::new(
            DecodeTarget::StreamChunk,
            "successful SSE response did not contain events".into(),
        )
        .into());
    };
    Ok(decode_stream(stream))
}

fn json_request<B: Serialize>(
    endpoint: &str,
    body: &B,
    accept: &str,
) -> Result<TransportRequest, BridgeError> {
    let body = serde_json::to_vec(body)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonRequest, Box::new(source)))?;
    Ok(TransportRequest {
        method: TransportMethod::Post,
        endpoint: endpoint.to_owned(),
        accept: Some(accept.to_owned()),
        body: Some(body),
    })
}

fn checked_body(response: TransportResponse, endpoint: &str) -> Result<Vec<u8>, BridgeError> {
    if (200..300).contains(&response.status) {
        Ok(response.body)
    } else {
        Err(map_status_error(
            response.status,
            &response.headers,
            &response.body,
            endpoint,
        ))
    }
}

fn decode_stream(mut input: TransportSseStream) -> crate::ImageStreamResult {
    Box::pin(async_stream::stream! {
        while let Some(item) = input.next().await {
            match item {
                Ok(event) => yield serde_json::from_str::<ImageStreamChunk>(&event.data)
                    .map_err(|source| DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)).into()),
                Err(error) => yield Err(error.into()),
            }
        }
    })
}

pub fn map_status_error(
    status: u16,
    headers: &[TransportHeader],
    body: &[u8],
    endpoint: &str,
) -> BridgeError {
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
    .into()
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
