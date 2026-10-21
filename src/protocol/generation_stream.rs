use futures_util::StreamExt;

use crate::{DecodeError, DecodeTarget, ImageStreamChunk, ImageStreamResult, TransportSseStream};

pub(super) fn decode(mut input: TransportSseStream) -> ImageStreamResult {
    Box::pin(async_stream::try_stream! {
        while let Some(event) = input.next().await {
            let event = event?;
            let chunk = serde_json::from_str::<ImageStreamChunk>(&event.data)
                .map_err(|source| DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)))?;
            yield chunk;
        }
    })
}
