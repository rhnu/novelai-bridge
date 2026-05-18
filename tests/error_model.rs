use std::{error::Error, time::Duration};

use novelai_bridge::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DecodeError, DecodeTarget, InvalidRequest,
};

#[test]
fn invalid_request_payload_is_matchable_and_self_describing() {
    let error: BridgeError = InvalidRequest::EmptyField {
        field: "prompt".to_owned(),
    }
    .into();

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if field == "prompt"
    ));
    assert_eq!(error.to_string(), "prompt cannot be empty");
}

#[test]
fn api_rate_limit_payload_uses_duration() {
    let error = BridgeError::Api(ApiError {
        kind: ApiErrorKind::RateLimited {
            retry_after: Some(Duration::from_secs(5)),
        },
        status: 429,
        endpoint: "https://image.novelai.net/ai/generate-image".to_owned(),
        server_reason: Some(ApiErrorReason::Message("limit".to_owned())),
        raw_body: Some("limit".to_owned()),
    });

    assert!(matches!(
        error,
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
fn api_error_display_includes_server_reason() {
    let error = BridgeError::Api(ApiError {
        kind: ApiErrorKind::InvalidRequest,
        status: 400,
        endpoint: "https://image.novelai.net/ai/generate-image".to_owned(),
        server_reason: Some(ApiErrorReason::Message("bad request shape".to_owned())),
        raw_body: Some("{\"message\":\"bad request shape\"}".to_owned()),
    });

    let display = error.to_string();
    assert!(display.contains("bad request shape"));
    assert!(display.contains("HTTP 400"));
}

#[test]
fn wrapped_decode_error_exposes_source_chain() {
    let source = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
    let error: BridgeError = DecodeError {
        target: DecodeTarget::JsonResponse,
        source: Box::new(source),
    }
    .into();

    let source = error.source().expect("decode source");
    assert!(source.to_string().contains("expected ident"));
}
