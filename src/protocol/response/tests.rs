use super::*;
use crate::ApiErrorKind;

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
        false,
    );
    assert_eq!(error.status(), 429);
    assert!(
        matches!(error.kind(), ApiErrorKind::RateLimited { retry_after: Some(delay) }
        if *delay == Duration::from_secs(5))
    );
    assert!(
        matches!(&error.server_reason, Some(ApiErrorReason::Message(message)) if message == "slow down")
    );
}

#[test]
fn truncated_error_body_preserves_status_and_reports_incomplete_payload() {
    let error = map_status_error(
        500,
        &[],
        br#"{"message":"partial"#,
        "https://example.test/generate",
        true,
    );
    assert_eq!(error.status(), 500);
    assert!(error.raw_body_truncated);
    assert!(error.server_reason.is_none());
    assert!(error.raw_body.is_some());
}

#[test]
fn invalid_retry_after_is_ignored() {
    assert_eq!(parse_retry_after(Some("-1"), SystemTime::UNIX_EPOCH), None);
    assert_eq!(
        parse_retry_after(Some("invalid"), SystemTime::UNIX_EPOCH),
        None
    );
}
