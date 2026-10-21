use async_trait::async_trait;
use novelai_bridge::{
    StreamingTransport, Transport, TransportError, TransportOperation, TransportRequest,
    TransportResponse, TransportSseResponse,
};
use serde_json::Value;

use crate::{TaskResult, render};

pub struct RequestEdits {
    overrides: Vec<(String, Value)>,
    removals: Vec<String>,
}

impl RequestEdits {
    pub fn new(overrides: &[String], removals: Vec<String>) -> TaskResult<Self> {
        Ok(Self {
            overrides: overrides
                .iter()
                .map(|raw| render::parse_override(raw))
                .collect::<Result<_, _>>()?,
            removals,
        })
    }

    pub fn apply(&self, mut request: TransportRequest) -> Result<TransportRequest, TransportError> {
        println!("{:?} {}", request.method, request.endpoint);
        if let Some(body) = request.body.take() {
            let rewrite = || -> TaskResult<Vec<u8>> {
                let mut value: Value = serde_json::from_slice(&body)?;
                for (path, replacement) in &self.overrides {
                    if !render::set_path(&mut value, path, replacement.clone()) {
                        return Err(format!("--set path does not exist: {path}").into());
                    }
                }
                for path in &self.removals {
                    if !render::remove_path(&mut value, path) {
                        return Err(format!("--unset path does not exist: {path}").into());
                    }
                }
                println!("{}", render::to_readable_json(&value));
                Ok(serde_json::to_vec(&value)?)
            };
            request.body = Some(rewrite().map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    source,
                )
            })?);
        }
        Ok(request)
    }
}

pub struct CaptureTransport<T> {
    inner: T,
    edits: RequestEdits,
}

impl<T> CaptureTransport<T> {
    pub const fn new(inner: T, edits: RequestEdits) -> Self { Self { inner, edits } }
}

#[async_trait]
impl<T: Transport> Transport for CaptureTransport<T> {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        self.inner.execute(self.edits.apply(request)?).await
    }
}

#[async_trait]
impl<T: StreamingTransport> StreamingTransport for CaptureTransport<T> {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        self.inner.execute_sse(self.edits.apply(request)?).await
    }
}
