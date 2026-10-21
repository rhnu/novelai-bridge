use std::sync::Mutex;

use async_trait::async_trait;
use futures_util::{StreamExt, stream};

use super::{ApiEndpoint, post_encoded, post_sse};
use crate::{
    ApiErrorKind, BridgeError, ResponseLimits, Transport, TransportError, TransportHeader,
    TransportRequest, TransportResponse,
};

struct Backend(Mutex<Option<TransportResponse>>);

#[async_trait]
impl Transport for Backend {
    async fn execute(&self, _: TransportRequest) -> Result<TransportResponse, TransportError> {
        Ok(self
            .0
            .lock()
            .expect("test response mutex is not poisoned")
            .take()
            .expect("one response was configured"))
    }
}

fn backend(status: u16, headers: Vec<TransportHeader>, chunks: Vec<Vec<u8>>) -> Backend {
    Backend(Mutex::new(Some(TransportResponse {
        status,
        headers,
        body: Box::pin(stream::iter(chunks.into_iter().map(Ok))),
    })))
}

fn header(name: &str, value: &str) -> TransportHeader {
    TransportHeader {
        name: name.to_owned(),
        value: value.as_bytes().to_vec(),
    }
}

fn sse(chunks: Vec<Vec<u8>>) -> Backend {
    backend(
        200,
        vec![header("Content-Type", "Text/Event-Stream; charset=utf-8")],
        chunks,
    )
}

fn request() -> TransportRequest {
    ApiEndpoint::GenerateImageStream.request("https://example.test", Some(Vec::new()))
}

#[tokio::test]
async fn an_http_error_preserves_api_details_before_stream_decoding() {
    let backend = backend(
        429,
        vec![header("Retry-After", "5")],
        vec![br#"{"message":"slow down"}"#.to_vec()],
    );
    let result = post_sse(&backend, request(), ResponseLimits::default()).await;
    assert!(matches!(result, Err(BridgeError::Api(api))
        if matches!(api.kind(), ApiErrorKind::RateLimited { .. })
            && api.server_reason.as_ref().is_some_and(|reason| reason.as_str() == "slow down")));
}

#[tokio::test]
async fn a_successful_ordinary_http_body_is_not_accepted_as_an_event_stream() {
    let backend = backend(200, vec![], vec![b"{}".to_vec()]);
    assert!(matches!(
        post_sse(&backend, request(), ResponseLimits::default()).await,
        Err(BridgeError::Decode(_))
    ));
}

#[tokio::test]
async fn malformed_chunk_terminates_the_decoded_stream() {
    let backend = sse(vec![b"data: not JSON\n\ndata: {}\n\n".to_vec()]);
    let mut output = post_sse(&backend, request(), ResponseLimits::default())
        .await
        .unwrap();
    assert!(matches!(
        output.next().await,
        Some(Err(BridgeError::Stream(_)))
    ));
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn unknown_event_types_cross_the_wire_boundary_without_losing_their_source() {
    let backend = sse(vec![
        b"event: future-event\nid: sample-1\nretry: 2500\ndata: arbitrary\n\n".to_vec(),
    ]);
    let mut output = post_sse(&backend, request(), ResponseLimits::default())
        .await
        .unwrap();
    let chunk = output.next().await.unwrap().unwrap();
    assert!(
        matches!(chunk.event_type(), crate::ImageStreamEventType::Unknown(ref name) if name == "future-event")
    );
    assert_eq!(chunk.raw.id, "sample-1");
    assert_eq!(
        chunk.raw.retry,
        Some(std::time::Duration::from_millis(2500))
    );
    assert_eq!(chunk.raw.data, "arbitrary");
}

#[tokio::test]
async fn sse_framing_handles_every_byte_split_and_multiple_events() {
    let bytes = "event: extension\ndata: è\n\nevent: extension\ndata:\n\n".as_bytes();
    let backend = sse(bytes.iter().map(|byte| vec![*byte]).collect());
    let mut output = post_sse(&backend, request(), ResponseLimits::default())
        .await
        .unwrap();
    assert_eq!(output.next().await.unwrap().unwrap().raw.data, "è");
    assert_eq!(output.next().await.unwrap().unwrap().raw.data, "");
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn success_limits_apply_to_custom_transport_and_preserve_headers() {
    let limits = ResponseLimits {
        success_body_bytes: 4,
        ..ResponseLimits::default()
    };
    let valid = backend(
        200,
        vec![header("X-Operation", "image")],
        vec![b"ab".to_vec(), b"cd".to_vec()],
    );
    let response = post_encoded(&valid, request(), limits).await.unwrap();
    assert_eq!(response.body, b"abcd");
    assert_eq!(response.header("x-operation"), Some(b"image".as_slice()));
    let oversized = backend(200, vec![], vec![b"abcd".to_vec(), b"e".to_vec()]);
    assert!(matches!(
        post_encoded(&oversized, request(), limits).await,
        Err(BridgeError::Decode(_))
    ));
}

#[tokio::test]
async fn error_limits_keep_only_a_prefix_and_report_real_truncation() {
    let limits = ResponseLimits {
        error_body_bytes: 4,
        ..ResponseLimits::default()
    };
    for (chunks, truncated) in [
        (vec![b"abcd".to_vec()], false),
        (vec![b"ab".to_vec(), b"cdef".to_vec()], true),
    ] {
        let backend = backend(500, vec![], chunks);
        let Err(BridgeError::Api(error)) = post_encoded(&backend, request(), limits).await else {
            panic!("non-success status must remain an API error");
        };
        assert_eq!(error.raw_body.as_deref(), Some("abcd"));
        assert_eq!(error.raw_body_truncated, truncated);
    }
}

#[tokio::test]
async fn sse_limits_apply_before_framing_for_custom_transports() {
    let backend = sse(vec![b"data: ".to_vec(), b"oversized".to_vec()]);
    let limits = ResponseLimits {
        sse_event_bytes: 8,
        ..ResponseLimits::default()
    };
    let mut output = post_sse(&backend, request(), limits).await.unwrap();
    let error = output.next().await.unwrap().unwrap_err();
    assert!(matches!(error, BridgeError::Decode(_)));
    assert!(output.next().await.is_none());
}

#[test]
fn buffered_response_debug_retains_only_content_free_summary() {
    let response = super::BufferedResponse {
        status: 200,
        headers: vec![header("X-Private", "private-header")],
        body: b"private-body".to_vec(),
    };
    let debug = format!("{response:?}");
    assert!(!debug.contains("private-header"));
    assert!(!debug.contains("private-body"));
    assert!(debug.contains("body_bytes"));
}

#[tokio::test]
async fn network_failures_preserve_transport_classification_and_cause() {
    let body = stream::iter([Err(TransportError::new(
        crate::TransportOperation::ReadResponseBytes,
        Some("https://example.test".to_owned()),
        Box::new(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "connection reset",
        )),
    ))]);
    let backend = Backend(Mutex::new(Some(TransportResponse {
        status: 200,
        headers: vec![header("Content-Type", "text/event-stream")],
        body: Box::pin(body),
    })));
    let mut output = post_sse(&backend, request(), ResponseLimits::default())
        .await
        .unwrap();
    let Err(BridgeError::Transport(error)) = output.next().await.unwrap() else {
        panic!("raw network errors must remain transport failures");
    };
    assert!(error.source.downcast_ref::<std::io::Error>().is_some());
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn account_body_limit_reports_the_shared_http_boundary() {
    let backend = backend(
        200,
        vec![header("Content-Type", "application/json")],
        vec![b"{}".to_vec()],
    );
    let limits = ResponseLimits {
        success_body_bytes: 1,
        ..ResponseLimits::default()
    };
    let result: Result<serde_json::Value, BridgeError> = super::get_json(
        &backend,
        ApiEndpoint::Subscription.request("https://example.test", None),
        limits,
    )
    .await;
    assert!(matches!(result, Err(BridgeError::Decode(error))
        if error.target == crate::DecodeTarget::HttpResponseBody
            && error.source.downcast_ref::<crate::ResponseLimitExceeded>().is_some()));
}
