use std::{error::Error, time::Duration};

use crate::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DecodeError, DecodeTarget, EncodeError,
    EncodeTarget, GenerationField, InvalidRequest, RequestField,
};

#[test]
fn invalid_request_payload_is_matchable_and_self_describing() {
    let error: BridgeError = InvalidRequest::EmptyField {
        field: GenerationField::Prompt.into(),
    }
    .into();

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if *field == RequestField::Generation(GenerationField::Prompt)
    ));
}

#[test]
fn request_encoding_failure_retains_the_original_error() {
    let source = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "synthetic encoding failure",
    );
    let error: BridgeError = EncodeError::new(EncodeTarget::JsonRequest, Box::new(source)).into();
    assert!(
        matches!(&error, BridgeError::Encode(encode) if encode.target == EncodeTarget::JsonRequest)
    );
    assert!(
        error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
}

#[test]
fn api_rate_limit_payload_uses_duration() {
    let api = ApiError::from_response_parts(
        429,
        "https://image.novelai.net/ai/generate-image",
        Some(Duration::from_secs(5)),
        Some(ApiErrorReason::Message("limit".to_owned())),
        Some("limit".to_owned()),
        true,
    );
    assert_eq!(api.status(), 429);
    assert!(api.raw_body_truncated);
    assert!(
        matches!(api.kind(), ApiErrorKind::RateLimited { retry_after: Some(delay) } if *delay == Duration::from_secs(5))
    );
}

#[test]
fn api_error_display_includes_server_reason() {
    let mut api = ApiError::new(400, "https://image.novelai.net/ai/generate-image");
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

#[test]
fn preparation_failure_is_distinct_from_invalid_input_and_retains_source() {
    let error: BridgeError = crate::PreparationError::SeedGenerationFailed {
        source: Box::new(std::io::Error::other("entropy source unavailable")),
    }
    .into();
    assert!(matches!(error, BridgeError::Preparation(_)));
    assert!(error.source().unwrap().is::<std::io::Error>());
}
