use super::*;

#[test]
fn retry_after_and_error_body_are_interpreted_above_transport() {
    let error = map_status_error(
        429,
        &[TransportHeader {
            name: "Retry-After".to_owned(),
            value: b"5".to_vec(),
        }],
        br#"{"message":"slow down"}"#,
        "https://example.test/generate",
    );
    assert!(matches!(error, ApiError {
        kind: ApiErrorKind::RateLimited { retry_after: Some(delay) },
        status: 429,
        server_reason: Some(ApiErrorReason::Message(ref message)),
        ..
    } if delay == Duration::from_secs(5) && message == "slow down"));
}

#[test]
fn invalid_retry_after_is_ignored() {
    assert_eq!(parse_retry_after(Some("-1"), SystemTime::UNIX_EPOCH), None);
    assert_eq!(
        parse_retry_after(Some("invalid"), SystemTime::UNIX_EPOCH),
        None
    );
}
