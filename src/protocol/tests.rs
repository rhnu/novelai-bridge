use std::sync::Mutex;

use async_trait::async_trait;
use futures_util::{StreamExt, stream};

use super::{ApiEndpoint, post_sse};
use crate::{
    ApiErrorKind, BridgeError, StreamingTransport, Transport, TransportError, TransportHeader,
    TransportRequest, TransportResponse, TransportSseEvent, TransportSseResponse,
};

struct Backend(Mutex<Option<TransportSseResponse>>);

#[async_trait]
impl Transport for Backend {
    async fn execute(&self, _: TransportRequest) -> Result<TransportResponse, TransportError> {
        panic!("SSE protocol must use the streaming capability")
    }
}

#[async_trait]
impl StreamingTransport for Backend {
    async fn execute_sse(
        &self,
        _: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        Ok(self
            .0
            .lock()
            .expect("test response mutex is not poisoned")
            .take()
            .expect("one response was configured"))
    }
}

#[tokio::test]
async fn an_http_error_preserves_api_details_before_stream_decoding() {
    let backend = Backend(Mutex::new(Some(TransportSseResponse::Http(
        TransportResponse {
            body_truncated: false,
            status: 429,
            headers: vec![TransportHeader {
                name: "retry-after".to_owned(),
                value: b"5".to_vec(),
            }],
            body: br#"{"message":"slow down"}"#.to_vec(),
        },
    ))));
    let result = post_sse(
        &backend,
        ApiEndpoint::GenerateImageStream.request("https://example.test", Some(Vec::new())),
    )
    .await;
    assert!(matches!(result, Err(BridgeError::Api(api))
        if matches!(api.kind(), ApiErrorKind::RateLimited { .. })
            && api.server_reason.as_ref().is_some_and(|reason| reason.as_str() == "slow down")));
}

#[tokio::test]
async fn a_successful_ordinary_http_body_is_not_accepted_as_an_event_stream() {
    let backend = Backend(Mutex::new(Some(TransportSseResponse::Http(
        TransportResponse {
            body_truncated: false,
            status: 200,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        },
    ))));
    assert!(matches!(
        post_sse(
            &backend,
            ApiEndpoint::GenerateImageStream.request("https://example.test", Some(Vec::new()))
        )
        .await,
        Err(BridgeError::Decode(_))
    ));
}

#[tokio::test]
async fn malformed_chunk_terminates_the_decoded_stream() {
    let backend = Backend(Mutex::new(Some(TransportSseResponse::Events(Box::pin(
        stream::iter([
            Ok(TransportSseEvent {
                data: "not JSON".to_owned(),
            }),
            Ok(TransportSseEvent {
                data: "{}".to_owned(),
            }),
        ]),
    )))));
    let mut output = post_sse(
        &backend,
        ApiEndpoint::GenerateImageStream.request("https://example.test", Some(Vec::new())),
    )
    .await
    .unwrap();
    assert!(matches!(
        output.next().await,
        Some(Err(BridgeError::Decode(_)))
    ));
    assert!(output.next().await.is_none());
}

#[tokio::test]
async fn unknown_event_types_cross_the_wire_boundary_without_losing_their_name() {
    let backend = Backend(Mutex::new(Some(TransportSseResponse::Events(Box::pin(
        stream::iter([Ok(TransportSseEvent {
            data: r#"{"event_type":"future-event","samp_ix":1,"gen_id":2,"image":""}"#.to_owned(),
        })]),
    )))));
    let mut output = post_sse(
        &backend,
        ApiEndpoint::GenerateImageStream.request("https://example.test", Some(Vec::new())),
    )
    .await
    .unwrap();
    let chunk = output.next().await.unwrap().unwrap();
    assert!(
        matches!(chunk.event_type, crate::ImageStreamEventType::Unknown(ref name) if name == "future-event")
    );
}
