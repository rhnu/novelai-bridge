use std::pin::Pin;

use base64::Engine;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    BridgeError, ImageDecodePolicy, ImageStreamChunk, ImageStreamError, ImageStreamEvent,
    TransportSseEvent, TransportSseStream,
};

pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

#[derive(Deserialize)]
struct IntermediateBody {
    samp_ix: u32,
    step_ix: u32,
    gen_id: u32,
    sigma: Option<f32>,
    image: String,
}

#[derive(Deserialize)]
struct FinalBody {
    samp_ix: u32,
    gen_id: u32,
    image: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    samp_ix: Option<u32>,
    gen_id: Option<u32>,
    message: String,
}

fn invalid(
    raw: TransportSseEvent,
    source: impl std::error::Error + Send + Sync + 'static,
) -> BridgeError {
    ImageStreamError::InvalidEvent {
        raw: Box::new(raw),
        source: Box::new(source),
    }
    .into()
}

fn parse<T: DeserializeOwned>(raw: &TransportSseEvent) -> Result<T, BridgeError> {
    serde_json::from_str(&raw.data).map_err(|source| invalid(raw.clone(), source))
}

fn decode_final(raw: &TransportSseEvent) -> Result<ImageStreamEvent, BridgeError> {
    let body: FinalBody = parse(raw)?;
    let image = base64::engine::general_purpose::STANDARD
        .decode(&body.image)
        .map_err(|source| ImageStreamError::InvalidFinal {
            samp_ix: body.samp_ix,
            raw: Box::new(raw.clone()),
            source: Box::new(source),
        })?;
    let image = crate::image::validated_generated_image(image, None, ImageDecodePolicy::default())
        .map_err(|source| ImageStreamError::InvalidFinal {
            samp_ix: body.samp_ix,
            raw: Box::new(raw.clone()),
            source: Box::new(source),
        })?;
    Ok(ImageStreamEvent::Final {
        samp_ix: body.samp_ix,
        gen_id: body.gen_id,
        image,
    })
}

/// Official event identities must agree between SSE and JSON. Unknown names may use an
/// arbitrary payload; malformed known events must never be downgraded to Unknown.
fn decode_event(raw: TransportSseEvent) -> ImageStreamItem {
    let header = (!raw.event.is_empty() && raw.event != "message").then_some(raw.event.as_str());
    let json = serde_json::from_str::<Value>(&raw.data);
    // An extension may define an entirely different payload schema, even reusing this key.
    // A recognizable official JSON discriminator still cannot be hidden by a conflicting header.
    if let Some(name) = header
        && !matches!(name, "intermediate" | "final" | "error")
        && !json
            .as_ref()
            .ok()
            .and_then(|value| value.get("event_type"))
            .and_then(Value::as_str)
            .is_some_and(|name| matches!(name, "intermediate" | "final" | "error"))
    {
        return Ok(ImageStreamChunk {
            event: ImageStreamEvent::Unknown {
                event_type: name.to_owned(),
            },
            raw,
        });
    }
    let discriminator = match json.as_ref().ok().and_then(|value| value.get("event_type")) {
        Some(Value::String(name)) => Some(name.as_str()),
        Some(_) => {
            return Err(invalid(
                raw,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "event_type must be a string",
                ),
            ));
        },
        None => None,
    };
    if let (Some(header), Some(discriminator)) = (header, discriminator)
        && header != discriminator
    {
        return Err(invalid(
            raw,
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "SSE event name disagrees with JSON event_type",
            ),
        ));
    }
    let Some(name) = discriminator.or(header).map(str::to_owned) else {
        return Err(match json {
            Err(source) => invalid(raw, source),
            Ok(_) => invalid(
                raw,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream event has no discriminator",
                ),
            ),
        });
    };
    let event = match name.as_str() {
        "intermediate" => {
            let body: IntermediateBody = parse(&raw)?;
            ImageStreamEvent::Intermediate {
                samp_ix: body.samp_ix,
                step_ix: body.step_ix,
                gen_id: body.gen_id,
                sigma: body.sigma,
                image: body.image,
            }
        },
        "final" => decode_final(&raw)?,
        "error" => {
            let body: ErrorBody = parse(&raw)?;
            ImageStreamEvent::Error {
                samp_ix: body.samp_ix,
                gen_id: body.gen_id,
                message: body.message,
            }
        },
        _ => ImageStreamEvent::Unknown { event_type: name },
    };
    let chunk = ImageStreamChunk { event, raw };
    if matches!(chunk.event, ImageStreamEvent::Error { .. }) {
        return Err(ImageStreamError::Provider {
            event: Box::new(chunk),
        }
        .into());
    }
    Ok(chunk)
}

pub(super) fn decode(mut input: TransportSseStream) -> ImageStreamResult {
    Box::pin(async_stream::try_stream! {
        while let Some(event) = input.next().await {
            yield decode_event(event?)?;
        }
    })
}

#[cfg(test)]
mod tests;
