//! A [`Transport`] wrapper that shows, rewrites and optionally suppresses outgoing JSON.

use async_trait::async_trait;
use novelai_bridge::{
    ReqwestTransport, StreamingTransport, Transport, TransportError, TransportOperation,
    TransportRequest, TransportResponse, TransportSseResponse,
};
use serde_json::Value;

use crate::render;

pub struct CaptureTransport {
    inner: Option<ReqwestTransport>,
    overrides: Vec<(String, Value)>,
    removals: Vec<String>,
}

impl CaptureTransport {
    pub const fn live(
        inner: ReqwestTransport,
        overrides: Vec<(String, Value)>,
        removals: Vec<String>,
    ) -> Self {
        Self {
            inner: Some(inner),
            overrides,
            removals,
        }
    }

    pub const fn dry_run(overrides: Vec<(String, Value)>, removals: Vec<String>) -> Self {
        Self {
            inner: None,
            overrides,
            removals,
        }
    }

    fn prepare(&self, mut request: TransportRequest) -> Result<TransportRequest, TransportError> {
        println!("{:?} {}", request.method, request.endpoint);
        if let Some(body) = request.body.as_ref() {
            let mut value = serde_json::from_slice::<Value>(body).map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?;
            for (path, replacement) in &self.overrides {
                if !render::set_path(&mut value, path, replacement.clone()) {
                    println!("  ! --set {path}: no such path in this request, ignored");
                }
            }
            for path in &self.removals {
                if !render::remove_path(&mut value, path) {
                    println!("  ! --unset {path}: no such path in this request, ignored");
                }
            }
            println!("{}", render::to_readable_json(&value));
            request.body = Some(serde_json::to_vec(&value).map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    Box::new(source),
                )
            })?);
        }
        Ok(request)
    }

    fn dry_run_error(endpoint: &str) -> TransportError {
        TransportError::new(
            TransportOperation::SendRequest,
            Some(endpoint.to_owned()),
            "dry run: request was assembled but not sent".into(),
        )
    }
}

#[async_trait]
impl Transport for CaptureTransport {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let request = self.prepare(request)?;
        match self.inner.as_ref() {
            Some(inner) => inner.execute(request).await,
            None => Err(Self::dry_run_error(&request.endpoint)),
        }
    }
}

#[async_trait]
impl StreamingTransport for CaptureTransport {
    async fn execute_sse(
        &self,
        request: TransportRequest,
    ) -> Result<TransportSseResponse, TransportError> {
        let request = self.prepare(request)?;
        match self.inner.as_ref() {
            Some(inner) => inner.execute_sse(request).await,
            None => Err(Self::dry_run_error(&request.endpoint)),
        }
    }
}
