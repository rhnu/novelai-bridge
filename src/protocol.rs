use serde::de::DeserializeOwned;

use crate::{BridgeError, DecodeError, DecodeTarget, Transport, TransportRequest};

mod endpoint;
pub mod limits;
mod response;
mod sse;
pub use endpoint::ApiEndpoint;
pub use response::BufferedResponse;
mod generation_stream;
pub use generation_stream::{ImageStreamItem, ImageStreamResult};

/// Executes a JSON account query and validates status before deserialization.
pub async fn get_json<T: Transport + ?Sized, Response: DeserializeOwned>(
    transport: &T,
    request: TransportRequest,
    limits: limits::ResponseLimits,
) -> Result<Response, BridgeError> {
    let response = post_encoded(transport, request, limits).await?;
    serde_json::from_slice(&response.body)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)).into())
}

/// Buffers a bounded ordinary response, preserving its headers for the operation decoder.
pub async fn post_encoded<T: Transport + ?Sized>(
    transport: &T,
    request: TransportRequest,
    limits: limits::ResponseLimits,
) -> Result<BufferedResponse, BridgeError> {
    let endpoint = request.endpoint.clone();
    let response = transport.execute(request).await?;
    response::checked_body(response, &endpoint, limits).await
}

pub async fn post_sse<T: Transport + ?Sized>(
    transport: &T,
    request: TransportRequest,
    limits: limits::ResponseLimits,
) -> Result<ImageStreamResult, BridgeError> {
    let endpoint = request.endpoint.clone();
    let response = transport.execute(request).await?;
    if !(200..300).contains(&response.status) {
        response::checked_body(response, &endpoint, limits).await?;
        unreachable!("non-success responses always fail status validation");
    }
    if !sse::is_event_stream(&response.headers) {
        return Err(DecodeError::new(
            DecodeTarget::StreamChunk,
            "successful stream request did not return text/event-stream".into(),
        )
        .into());
    }
    Ok(generation_stream::decode(sse::decode(
        response.body,
        limits.sse_event_bytes,
    )))
}

#[cfg(test)]
mod tests;
