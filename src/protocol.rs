use serde::de::DeserializeOwned;

use crate::{BridgeError, DecodeError, DecodeTarget, Transport, TransportMethod, TransportRequest};
#[cfg(feature = "streaming")]
use crate::{StreamingTransport, TransportSseResponse};

mod endpoint;
mod response;
pub use endpoint::Endpoint;
#[cfg(feature = "streaming")]
mod generation_stream;

/// Executes a JSON account query and validates the HTTP status before deserialization.
pub async fn get_json<T: Transport + ?Sized, Response: DeserializeOwned>(
    transport: &T,
    endpoint: &str,
) -> Result<Response, BridgeError> {
    let response = transport
        .execute(TransportRequest {
            method: TransportMethod::Get,
            endpoint: endpoint.to_owned(),
            accept: Some("application/json".to_owned()),
            body: None,
        })
        .await?;
    let body = response::checked_body(response, endpoint)?;
    serde_json::from_slice(&body)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)).into())
}

pub async fn post_encoded<T: Transport + ?Sized>(
    transport: &T,
    endpoint: &str,
    body: Vec<u8>,
) -> Result<Vec<u8>, BridgeError> {
    let response = transport
        .execute(encoded_request(endpoint, body, "application/octet-stream"))
        .await?;
    Ok(response::checked_body(response, endpoint)?)
}

#[cfg(feature = "streaming")]
pub async fn post_sse<T: StreamingTransport + ?Sized>(
    transport: &T,
    endpoint: &str,
    body: Vec<u8>,
) -> Result<crate::ImageStreamResult, BridgeError> {
    match transport
        .execute_sse(encoded_request(endpoint, body, "text/event-stream"))
        .await?
    {
        TransportSseResponse::Events(events) => Ok(generation_stream::decode(events)),
        TransportSseResponse::Http(response) => {
            response::checked_body(response, endpoint)?;
            Err(DecodeError::new(
                DecodeTarget::StreamChunk,
                "successful SSE request returned an ordinary HTTP body".into(),
            )
            .into())
        },
    }
}

fn encoded_request(endpoint: &str, body: Vec<u8>, accept: &str) -> TransportRequest {
    TransportRequest {
        method: TransportMethod::Post,
        endpoint: endpoint.to_owned(),
        accept: Some(accept.to_owned()),
        body: Some(body),
    }
}

#[cfg(all(test, feature = "streaming"))]
mod tests;
