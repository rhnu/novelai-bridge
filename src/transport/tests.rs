use eventsource_stream::Eventsource;
use futures_util::{StreamExt, stream};

use super::*;

#[test]
fn retry_after_parses_seconds() {
    let parsed = parse_retry_after_ms(Some("15"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, Some(15_000));
}

#[test]
fn retry_after_negative_is_zero() {
    let parsed = parse_retry_after_ms(Some("-1"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, Some(0));
}

#[test]
fn retry_after_invalid_is_zero() {
    let parsed = parse_retry_after_ms(Some("abc"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, Some(0));
}

#[test]
fn map_429_includes_retry_after() {
    let mut headers = HeaderMap::new();
    headers.insert(reqwest::header::RETRY_AFTER, "5".parse().unwrap());
    let err = map_status_error(
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        &headers,
        "limit".to_owned(),
        "/ai/generate-image",
    );
    assert_eq!(err.code, "rate_limited");
    assert_eq!(err.retry_after_ms, Some(5000));
}

#[test]
fn map_400_uses_json_message_field() {
    let headers = HeaderMap::new();
    let err = map_status_error(
        reqwest::StatusCode::BAD_REQUEST,
        &headers,
        "{\"statusCode\":400,\"message\":\"Error encoding v4 director references: non-200 response: 400\"}"
            .to_owned(),
        "/ai/generate-image-stream",
    );
    assert_eq!(err.code, "invalid_request");
    assert_eq!(
        err.message,
        "Error encoding v4 director references: non-200 response: 400"
    );
}

#[tokio::test]
async fn sse_parser_handles_split_chunks() {
    let chunks = vec![
        Ok::<Vec<u8>, ()>(b"data: {\"event\":\"a\"}\n".to_vec()),
        Ok::<Vec<u8>, ()>(b"\n".to_vec()),
    ];
    let mut events = stream::iter(chunks).eventsource();
    let first = events.next().await.unwrap().unwrap();
    assert_eq!(first.data, "{\"event\":\"a\"}");
}

#[tokio::test]
async fn sse_parser_handles_multiple_events() {
    let chunks = vec![Ok::<Vec<u8>, ()>(b"data: one\n\ndata: two\n\n".to_vec())];
    let mut events = stream::iter(chunks).eventsource();
    let first = events.next().await.unwrap().unwrap();
    let second = events.next().await.unwrap().unwrap();
    assert_eq!(first.data, "one");
    assert_eq!(second.data, "two");
}
