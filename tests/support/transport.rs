use std::sync::{Arc, Mutex};

use async_trait::async_trait;
#[cfg(feature = "streaming")]
use futures_util::stream;
#[cfg(feature = "streaming")]
use novelai_bridge::{
    ImageStreamChunk, StreamingTransport, TransportSseEvent, TransportSseResponse,
};
use novelai_bridge::{
    Transport, TransportError, TransportMethod, TransportOperation, TransportRequest,
    TransportResponse,
};
use serde_json::Value;

use super::create_png_bytes;

#[derive(Clone)]
pub struct MockTransport {
    state: Arc<Mutex<MockTransportState>>,
}

#[derive(Default)]
struct MockTransportState {
    last_path: Option<String>,
    last_body: Option<Value>,
    post_history: Vec<(String, Value)>,
    byte_paths: Vec<String>,
    bytes_response: Vec<u8>,
    encode_vibe_response: Vec<u8>,
    #[cfg(feature = "streaming")]
    sse_chunks: Vec<ImageStreamChunk>,
    json_response: Value,
    bytes_error: Option<TransportError>,
}

impl Default for MockTransport {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockTransportState {
                bytes_response: create_png_bytes(2, 2),
                ..MockTransportState::default()
            })),
        }
    }
}

impl MockTransport {
    pub fn with_bytes_response(bytes_response: Vec<u8>) -> Self {
        let transport = Self::default();
        transport.set_bytes_response(bytes_response);
        transport
    }

    pub fn set_bytes_response(&self, bytes_response: Vec<u8>) {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .bytes_response = bytes_response;
    }

    pub fn set_encode_vibe_response(&self, encode_vibe_response: Vec<u8>) {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .encode_vibe_response = encode_vibe_response;
    }

    #[cfg(feature = "streaming")]
    pub fn set_sse_chunks(&self, sse_chunks: Vec<ImageStreamChunk>) {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .sse_chunks = sse_chunks;
    }

    pub fn set_json_response(&self, json_response: Value) {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .json_response = json_response;
    }

    /// Makes the next `post_bytes` call fail with `bytes_error`.
    #[cfg(feature = "credit-observer")]
    pub fn set_bytes_error(&self, bytes_error: TransportError) {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .bytes_error = Some(bytes_error);
    }

    pub fn last_body(&self) -> Option<Value> {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .last_body
            .clone()
    }

    pub fn last_path(&self) -> Option<String> {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .last_path
            .clone()
    }

    pub fn byte_paths(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .byte_paths
            .clone()
    }

    pub fn bodies_for_path(&self, path: &str) -> Vec<Value> {
        self.state
            .lock()
            .expect("mock state mutex is not poisoned")
            .post_history
            .iter()
            .filter(|(recorded_path, _)| recorded_path == path)
            .map(|(_, body)| body.clone())
            .collect()
    }
}

#[async_trait]
impl Transport for MockTransport {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let body = request
            .body
            .as_ref()
            .map(|body| serde_json::from_slice::<Value>(body))
            .transpose()
            .map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?;
        let mut state = self.state.lock().expect("mock state mutex is not poisoned");
        state.last_path = Some(request.endpoint.clone());
        if request.method == TransportMethod::Get {
            return Ok(TransportResponse {
                body_truncated: false,
                status: 200,
                headers: Vec::new(),
                body: serde_json::to_vec(&state.json_response)
                    .expect("mock JSON response is serializable"),
            });
        }
        if let Some(body) = body {
            state.last_body = Some(body.clone());
            state.post_history.push((request.endpoint.clone(), body));
        }
        state.byte_paths.push(request.endpoint.clone());
        if let Some(error) = state.bytes_error.take() {
            return Err(error);
        }
        let body = if request.endpoint.ends_with("/ai/encode-vibe") {
            state.encode_vibe_response.clone()
        } else {
            state.bytes_response.clone()
        };
        Ok(TransportResponse {
            body_truncated: false,
            status: 200,
            headers: Vec::new(),
            body,
        })
    }
}

#[cfg(feature = "streaming")]
#[async_trait]
impl StreamingTransport for MockTransport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        let body = serde_json::from_slice::<Value>(request.body.as_deref().unwrap_or_default())
            .map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?;
        let mut state = self.state.lock().expect("mock state mutex is not poisoned");
        state.last_path = Some(request.endpoint.clone());
        state.last_body = Some(body.clone());
        state.post_history.push((request.endpoint, body));
        let items: Vec<Result<TransportSseEvent, TransportError>> = state
            .sse_chunks
            .iter()
            .map(|chunk| Ok(chunk.raw.clone()))
            .collect();
        drop(state);
        Ok(TransportSseResponse::Events(Box::pin(stream::iter(items))))
    }
}
