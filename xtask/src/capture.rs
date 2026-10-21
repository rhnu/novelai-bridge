use async_trait::async_trait;
use novelai_bridge::{
    Transport, TransportError, TransportOperation, TransportRequest, TransportResponse,
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
                .map(|raw| render::parse_override(raw).map_err(anyhow::Error::msg))
                .collect::<Result<_, _>>()?,
            removals,
        })
    }

    pub fn apply(&self, mut request: TransportRequest) -> Result<TransportRequest, TransportError> {
        println!("{:?} {}", request.method, request.endpoint);
        if request
            .content_type
            .as_deref()
            .is_some_and(|value| value.starts_with("multipart/form-data"))
        {
            if !self.overrides.is_empty() || !self.removals.is_empty() {
                return Err(TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    "body edits require JSON request encoding".into(),
                ));
            }
            println!(
                "multipart body: {} bytes (media and cache keys omitted)",
                request.body.as_ref().map_or(0, Vec::len)
            );
            return Ok(request);
        }
        if request.body.is_none() && (!self.overrides.is_empty() || !self.removals.is_empty()) {
            return Err(TransportError::new(
                TransportOperation::SendRequest,
                Some(request.endpoint.clone()),
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "cannot apply body edits to an endpoint without a request body",
                )),
            ));
        }
        if let Some(body) = request.body.take() {
            let rewrite = || -> TaskResult<Vec<u8>> {
                let mut value: Value = serde_json::from_slice(&body)?;
                for (path, replacement) in &self.overrides {
                    if !render::set_path(&mut value, path, replacement.clone()) {
                        anyhow::bail!("--set parent path does not exist: {path}");
                    }
                }
                for path in &self.removals {
                    if !render::remove_path(&mut value, path) {
                        anyhow::bail!("--unset path does not exist: {path}");
                    }
                }
                println!("{}", render::to_readable_json(&value));
                Ok(serde_json::to_vec(&value)?)
            };
            request.body = Some(rewrite().map_err(|source| {
                TransportError::new(
                    TransportOperation::SendRequest,
                    Some(request.endpoint.clone()),
                    source.into_boxed_dyn_error(),
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
        let response = self.inner.execute(self.edits.apply(request)?).await?;
        println!(
            "HTTP {}, content-type {:?}",
            response.status,
            response
                .headers
                .iter()
                .find(|header| header.name.eq_ignore_ascii_case("content-type"))
                .map(|header| String::from_utf8_lossy(&header.value))
        );
        Ok(response)
    }
}

#[cfg(test)]
mod tests;
