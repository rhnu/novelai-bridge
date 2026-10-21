mod cache;
mod multipart;

pub(super) use cache::CacheState;

use super::Client;
use crate::{BridgeError, PreparationError, Transport, TransportRequest, TransportResponse};

/// Image-service body encoding. Both forms carry the same prepared semantic request.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ImageRequestEncoding {
    #[default]
    Json,
    Multipart,
}

/// Bounds local registration metadata; image bytes remain owned by the prepared request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ImageCacheConfig {
    pub max_entries: usize,
}

impl Default for ImageCacheConfig {
    fn default() -> Self { Self { max_entries: 128 } }
}

impl<T: Transport + ?Sized> Client<T> {
    pub(super) async fn execute_image_request(
        &self,
        mut request: TransportRequest,
        cacheable: bool,
    ) -> Result<TransportResponse, BridgeError> {
        let caching = cacheable
            && self
                .config
                .image_cache
                .is_some_and(|config| config.max_entries > 0);
        if !caching && self.config.image_request_encoding == ImageRequestEncoding::Json {
            return Ok(self.transport.execute(request).await?);
        }
        let Some(body) = request.body.take() else {
            return Ok(self.transport.execute(request).await?);
        };
        let value = serde_json::from_slice(&body).map_err(encode_error)?;
        let mut transaction = if caching {
            self.image_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .transform(value)?
        } else {
            cache::CacheTransaction::uncached(value)
        };
        encode_request(
            &mut request,
            &transaction.body,
            self.config.image_request_encoding,
        )?;
        if !caching {
            drop(transaction);
            return Ok(self.transport.execute(request).await?);
        }
        let endpoint = request.endpoint.clone();
        let outgoing_body = request.body.take();
        let outgoing = TransportRequest {
            body: outgoing_body,
            ..request.clone()
        };
        let mut response = self.transport.execute(outgoing).await?;
        if caching && response.status == 400 {
            // Inspect only an explicitly rejected request, using the shared bounded error read.
            // Ambiguous transport failures and other HTTP statuses are never retried.
            let error =
                crate::protocol::buffer_response(response, &endpoint, self.config.response_limits)
                    .await
                    .expect_err("HTTP 400 is a non-success response");
            let invalid = cache::invalid_keys(&error);
            let restored = invalid
                .as_ref()
                .is_some_and(|keys| transaction.restore(keys));
            if !restored {
                return Err(error);
            }
            if let Some(keys) = invalid {
                self.image_cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .invalidate(&keys);
            }
            encode_request(
                &mut request,
                &transaction.body,
                self.config.image_request_encoding,
            )?;
            response = self.transport.execute(request).await?;
            tracing::debug!("retried explicitly rejected image cache references");
        }
        if (200..300).contains(&response.status)
            && let Some(config) = self.config.image_cache
        {
            self.image_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .register(transaction.registrations, config.max_entries);
        }
        Ok(response)
    }
}

fn encode_request(
    request: &mut TransportRequest,
    value: &serde_json::Value,
    encoding: ImageRequestEncoding,
) -> Result<(), BridgeError> {
    match encoding {
        ImageRequestEncoding::Json => {
            request.body = Some(serde_json::to_vec(value).map_err(encode_error)?);
            request.content_type = Some("application/json".to_owned());
        },
        ImageRequestEncoding::Multipart => {
            let (body, content_type) = multipart::encode(value)?;
            request.body = Some(body);
            request.content_type = Some(content_type);
        },
    }
    Ok(())
}

fn random_bytes<const N: usize>() -> Result<[u8; N], BridgeError> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|source| PreparationError::RequestRandomnessFailed {
        source: Box::new(source),
    })?;
    Ok(bytes)
}

fn encode_error(source: serde_json::Error) -> BridgeError {
    crate::EncodeError::new(crate::EncodeTarget::JsonRequest, Box::new(source)).into()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("writing to a String cannot fail");
    }
    result
}

#[cfg(test)]
mod tests;
