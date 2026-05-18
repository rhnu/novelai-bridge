use eventsource_stream::Eventsource;
use futures_util::{StreamExt, stream};

use super::*;

#[test]
fn retry_after_parses_seconds() {
    let parsed = parse_retry_after(Some("15"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, Some(Duration::from_secs(15)));
}

#[test]
fn retry_after_negative_is_none() {
    let parsed = parse_retry_after(Some("-1"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, None);
}

#[test]
fn retry_after_invalid_is_none() {
    let parsed = parse_retry_after(Some("abc"), SystemTime::UNIX_EPOCH);
    assert_eq!(parsed, None);
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
    assert!(matches!(
        err,
        BridgeError::Api(ApiError {
            kind: ApiErrorKind::RateLimited {
                retry_after: Some(delay),
            },
            status: 429,
            ..
        }) if delay == Duration::from_secs(5)
    ));
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
    assert!(matches!(
        err,
        BridgeError::Api(ApiError {
            kind: ApiErrorKind::InvalidRequest,
            status: 400,
            server_reason: Some(ApiErrorReason::Message(ref message)),
            ..
        }) if message == "Error encoding v4 director references: non-200 response: 400"
    ));
}

#[test]
fn image_stream_chunk_payload_parses_json_data() {
    let chunk = parse_stream_chunk(
        "{\"event_type\":\"step\",\"samp_ix\":0,\"step_ix\":1,\"gen_id\":1,\"sigma\":1.1,\"image\":\"abc\"}",
    )
    .unwrap();
    assert_eq!(chunk.event_type, "step");
    assert_eq!(chunk.step_ix, Some(1));
    assert_eq!(chunk.image, "abc");
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
