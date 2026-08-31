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
    let mut api = ApiError::new(
        ApiErrorKind::RateLimited {
            retry_after: Some(Duration::from_secs(5)),
        },
        429,
        "https://image.novelai.net/ai/generate-image",
    );
    api.server_reason = Some(ApiErrorReason::Message("limit".to_owned()));
    api.raw_body = Some("limit".to_owned());
    let error = BridgeError::Api(api);

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
    let mut api = ApiError::new(
        ApiErrorKind::InvalidRequest,
        400,
        "https://image.novelai.net/ai/generate-image",
    );
    api.server_reason = Some(ApiErrorReason::Message("bad request shape".to_owned()));
    api.raw_body = Some("{\"message\":\"bad request shape\"}".to_owned());
    let error = BridgeError::Api(api);

    let display = error.to_string();
    assert!(display.contains("bad request shape"));
    assert!(display.contains("HTTP 400"));
}

#[test]
fn wrapped_decode_error_exposes_source_chain() {
    let source = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
    let error: BridgeError = DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)).into();

    let source = error.source().expect("decode source");
    assert!(source.to_string().contains("expected ident"));
}
