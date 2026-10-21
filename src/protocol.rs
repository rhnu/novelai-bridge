use serde::de::DeserializeOwned;

use crate::{BridgeError, DecodeError, DecodeTarget, Transport, TransportRequest};
#[cfg(feature = "streaming")]
use crate::{StreamingTransport, TransportSseResponse};

mod endpoint;
mod response;
pub use endpoint::ApiEndpoint;
#[cfg(feature = "streaming")]
mod generation_stream;
#[cfg(feature = "streaming")]
pub use generation_stream::{ImageStreamItem, ImageStreamResult};

/// Executes a JSON account query and validates the HTTP status before deserialization.
pub async fn get_json<T: Transport + ?Sized, Response: DeserializeOwned>(
    transport: &T,
    request: TransportRequest,
) -> Result<Response, BridgeError> {
    let endpoint = request.endpoint.clone();
    let response = transport.execute(request).await?;
    let body = response::checked_body(response, &endpoint)?;
    serde_json::from_slice(&body)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)).into())
}

pub async fn post_encoded<T: Transport + ?Sized>(
    transport: &T,
    request: TransportRequest,
) -> Result<Vec<u8>, BridgeError> {
    let endpoint = request.endpoint.clone();
    let response = transport.execute(request).await?;
    Ok(response::checked_body(response, &endpoint)?)
}

#[cfg(feature = "streaming")]
pub async fn post_sse<T: StreamingTransport + ?Sized>(
    transport: &T,
    request: TransportRequest,
) -> Result<ImageStreamResult, BridgeError> {
    let endpoint = request.endpoint.clone();
    match transport.execute_sse(request).await? {
        TransportSseResponse::Events(events) => Ok(generation_stream::decode(events)),
        TransportSseResponse::Http(response) => {
            response::checked_body(response, &endpoint)?;
            Err(DecodeError::new(
                DecodeTarget::StreamChunk,
                "successful SSE request returned an ordinary HTTP body".into(),
            )
            .into())
        },
    }
}

#[cfg(all(test, feature = "streaming"))]
mod tests;
