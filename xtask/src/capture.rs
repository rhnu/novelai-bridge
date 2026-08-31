//! A [`Transport`] wrapper that shows, rewrites and optionally suppresses outgoing request bodies.

use async_trait::async_trait;
use novelai_bridge::{
    BridgeError, ReqwestTransport, Transport, TransportError, TransportOperation,
    TransportSseStream,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::render;

/// Wraps the real transport so a probe run can inspect exactly what the library assembled.
///
/// `overrides` are applied to the serialized body *after* the library built it, which is what
/// makes A/B testing an individual wire field a single command instead of a code change.
pub struct CaptureTransport {
    inner: Option<ReqwestTransport>,
    overrides: Vec<(String, Value)>,
    removals: Vec<String>,
}

impl CaptureTransport {
    /// Prints bodies, then forwards them to `inner`.
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

    /// Prints bodies, then fails without sending anything.
    pub const fn dry_run(overrides: Vec<(String, Value)>, removals: Vec<String>) -> Self {
        Self {
            inner: None,
            overrides,
            removals,
        }
    }

    fn prepare<TReq: Serialize>(&self, endpoint: &str, body: &TReq) -> Result<Value, BridgeError> {
        let mut value = serde_json::to_value(body).map_err(|source| {
            TransportError::new(
                TransportOperation::SendRequest,
                Some(endpoint.to_owned()),
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

        println!("POST {endpoint}");
        println!("{}", render::to_readable_json(&value));
        Ok(value)
    }

    fn dry_run_error(endpoint: &str) -> BridgeError {
        TransportError::new(
            TransportOperation::SendRequest,
            Some(endpoint.to_owned()),
            "dry run: request was assembled but not sent".into(),
        )
        .into()
    }
}

#[async_trait]
impl Transport for CaptureTransport {
    async fn get_json<TRes: DeserializeOwned + Send>(
        &self,
        endpoint: &str,
    ) -> Result<TRes, BridgeError> {
        println!("GET {endpoint}");
        match self.inner.as_ref() {
            Some(inner) => inner.get_json(endpoint).await,
            None => Err(Self::dry_run_error(endpoint)),
        }
    }

    async fn post_bytes<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<Vec<u8>, BridgeError> {
        let value = self.prepare(endpoint, body)?;
        match self.inner.as_ref() {
            Some(inner) => inner.post_bytes(endpoint, &value).await,
            None => Err(Self::dry_run_error(endpoint)),
        }
    }

    async fn post_sse<TReq: Serialize + Send + Sync>(
        &self,
        endpoint: &str,
        body: &TReq,
    ) -> Result<TransportSseStream, BridgeError> {
        let value = self.prepare(endpoint, body)?;
        match self.inner.as_ref() {
            Some(inner) => inner.post_sse(endpoint, &value).await,
            None => Err(Self::dry_run_error(endpoint)),
        }
    }
}
