use serde::de::DeserializeOwned;

use crate::{
    BridgeError, DecodeError, DecodeTarget, StreamMode, Transport, TransportRequest,
    TransportResponse,
};

mod endpoint;
pub mod limits;
mod msgpack;
mod response;
mod sse;
pub use endpoint::ApiEndpoint;
pub use response::{BufferedResponse, checked_body as buffer_response};
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

#[cfg(test)]
pub async fn post_sse<T: Transport + ?Sized>(
    transport: &T,
    request: TransportRequest,
    limits: limits::ResponseLimits,
) -> Result<ImageStreamResult, BridgeError> {
    post_stream(transport, request, StreamMode::Sse, limits).await
}

/// Opens an explicitly selected stream protocol from a raw HTTP transport.
#[cfg(test)]
pub async fn post_stream<T: Transport + ?Sized>(
    transport: &T,
    request: TransportRequest,
    mode: StreamMode,
    limits: limits::ResponseLimits,
) -> Result<ImageStreamResult, BridgeError> {
    let endpoint = request.endpoint.clone();
    let response = transport.execute(request).await?;
    decode_stream_response(response, &endpoint, mode, limits).await
}

/// Validates HTTP status and selects framing without sending another request.
pub async fn decode_stream_response(
    response: TransportResponse,
    endpoint: &str,
    mode: StreamMode,
    limits: limits::ResponseLimits,
) -> Result<ImageStreamResult, BridgeError> {
    if !(200..300).contains(&response.status) {
        response::checked_body(response, endpoint, limits).await?;
        unreachable!("non-success responses always fail status validation");
    }
    if mode == StreamMode::Sse && !sse::is_event_stream(&response.headers) {
        return Err(DecodeError::new(
            DecodeTarget::StreamChunk,
            "successful stream request did not return text/event-stream".into(),
        )
        .into());
    }
    if mode == StreamMode::MessagePack && sse::is_event_stream(&response.headers) {
        return Err(DecodeError::new(
            DecodeTarget::StreamChunk,
            "MessagePack request returned an SSE response".into(),
        )
        .into());
    }
    // The captured frontend selects MessagePack in the request body without an Accept
    // header. The live service returned application/msgpack; allow binary MIME variants while rejecting SSE.
    Ok(match mode {
        StreamMode::Sse => {
            generation_stream::decode_sse(sse::decode(response.body, limits.sse_event_bytes))
        },
        StreamMode::MessagePack => generation_stream::decode_msgpack(msgpack::decode(
            response.body,
            limits.msgpack_frame_bytes,
        )),
    })
}

#[cfg(test)]
mod tests;
