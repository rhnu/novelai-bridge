use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;
use serde::Deserialize;

use crate::{BridgeError, DecodeError, DecodeTarget, ImageStreamChunk, TransportSseStream};

pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

/// Provider JSON is decoded separately from the public event model.
#[derive(Deserialize)]
struct StreamChunkBody {
    event_type: String,
    samp_ix: u32,
    step_ix: Option<u32>,
    gen_id: u32,
    sigma: Option<f32>,
    image: String,
}

impl From<StreamChunkBody> for ImageStreamChunk {
    fn from(body: StreamChunkBody) -> Self {
        Self {
            event_type: body.event_type.into(),
            samp_ix: body.samp_ix,
            step_ix: body.step_ix,
            gen_id: body.gen_id,
            sigma: body.sigma,
            image: body.image,
        }
    }
}

pub(super) fn decode(mut input: TransportSseStream) -> ImageStreamResult {
    Box::pin(async_stream::try_stream! {
        while let Some(event) = input.next().await {
            let event = event?;
            let chunk = serde_json::from_str::<StreamChunkBody>(&event.data)
                .map_err(|source| DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)))?;
            yield chunk.into();
        }
    })
}
