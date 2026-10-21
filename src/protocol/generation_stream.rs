mod messagepack;

use std::{borrow::Cow, pin::Pin};

use base64::Engine;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{
    Deserialize,
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
};

use crate::{
    BridgeError, ImageDecodePolicy, ImageStreamChunk, ImageStreamError, ImageStreamEvent,
    TransportSseEvent,
};

pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

#[derive(Deserialize)]
struct IntermediateBody<'a> {
    samp_ix: u32,
    step_ix: u32,
    gen_id: Option<u32>,
    sigma: Option<f32>,
    #[serde(borrow)]
    image: Cow<'a, str>,
}

#[derive(Deserialize)]
struct FinalBody<'a> {
    samp_ix: u32,
    gen_id: Option<u32>,
    #[serde(borrow)]
    image: Cow<'a, str>,
}

#[derive(Deserialize)]
struct ErrorBody {
    samp_ix: Option<u32>,
    gen_id: Option<u32>,
    message: String,
}

enum Discriminator<'a> {
    Missing,
    Name(Cow<'a, str>),
    Other,
}

impl<'de> Deserialize<'de> for Discriminator<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DiscriminatorVisitor;

        impl<'de> Visitor<'de> for DiscriminatorVisitor {
            type Value = Discriminator<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an event discriminator")
            }

            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                Ok(Discriminator::Name(Cow::Borrowed(value)))
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Discriminator::Name(Cow::Owned(value.to_owned())))
            }

            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(Discriminator::Other)
            }

            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(Discriminator::Other)
            }

            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(Discriminator::Other)
            }

            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(Discriminator::Other)
            }

            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(Discriminator::Other)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut values: A) -> Result<Self::Value, A::Error> {
                while values.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Discriminator::Other)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> Result<Self::Value, A::Error> {
                while values.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Discriminator::Other)
            }
        }

        deserializer.deserialize_any(DiscriminatorVisitor)
    }
}

/// Reads only the identity; large image strings and extension objects are skipped in place.
fn discriminator(data: &str) -> Result<Discriminator<'_>, serde_json::Error> {
    struct IdentityVisitor;

    impl<'de> Visitor<'de> for IdentityVisitor {
        type Value = Discriminator<'de>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an event object")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut values: A) -> Result<Self::Value, A::Error> {
            let mut identity = Discriminator::Missing;
            while let Some(key) = values.next_key::<Cow<'de, str>>()? {
                if key == "event_type" {
                    identity = values.next_value()?;
                } else {
                    values.next_value::<IgnoredAny>()?;
                }
            }
            Ok(identity)
        }
    }

    // Non-object JSON has no discriminator, but still must be syntactically valid.
    if !data.trim_start().starts_with('{') {
        serde_json::from_str::<IgnoredAny>(data)?;
        return Ok(Discriminator::Missing);
    }
    let mut deserializer = serde_json::Deserializer::from_str(data);
    let identity = serde::Deserializer::deserialize_map(&mut deserializer, IdentityVisitor)?;
    deserializer.end()?;
    Ok(identity)
}

fn invalid(
    raw: TransportSseEvent,
    source: impl std::error::Error + Send + Sync + 'static,
) -> BridgeError {
    ImageStreamError::InvalidEvent {
        raw: Box::new(raw.into()),
        source: Box::new(source),
    }
    .into()
}

fn parse<'a, T: Deserialize<'a>>(raw: &'a TransportSseEvent) -> Result<T, BridgeError> {
    serde_json::from_str(&raw.data).map_err(|source| invalid(raw.clone(), source))
}

fn decode_final(raw: &TransportSseEvent) -> Result<ImageStreamEvent, BridgeError> {
    let body: FinalBody<'_> = parse(raw)?;
    let image = base64::engine::general_purpose::STANDARD
        .decode(body.image.as_bytes())
        .map_err(|source| ImageStreamError::InvalidFinal {
            samp_ix: body.samp_ix,
            raw: Box::new(raw.clone().into()),
            source: Box::new(source),
        })?;
    let image = crate::image::validated_generated_image(
        image,
        crate::GeneratedImageSource::Raw,
        ImageDecodePolicy::default(),
    )
    .map_err(|source| ImageStreamError::InvalidFinal {
        samp_ix: body.samp_ix,
        raw: Box::new(raw.clone().into()),
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
    let json = discriminator(&raw.data);
    // An extension may define an entirely different payload schema, even reusing this key.
    // A recognizable official JSON discriminator still cannot be hidden by a conflicting header.
    if let Some(name) = header
        && !matches!(name, "intermediate" | "final" | "error")
        && !json
            .as_ref()
            .is_ok_and(|value| matches!(value, Discriminator::Name(name) if matches!(name.as_ref(), "intermediate" | "final" | "error")))
    {
        return Ok(ImageStreamChunk {
            event: ImageStreamEvent::Unknown {
                event_type: name.to_owned(),
            },
            raw: raw.into(),
        });
    }
    let discriminator = match json.as_ref().ok() {
        Some(Discriminator::Name(name)) => Some(name.as_ref()),
        Some(Discriminator::Other) => {
            return Err(invalid(
                raw,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "event_type must be a string",
                ),
            ));
        },
        Some(Discriminator::Missing) | None => None,
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
            let body: IntermediateBody<'_> = parse(&raw)?;
            ImageStreamEvent::Intermediate {
                samp_ix: body.samp_ix,
                step_ix: body.step_ix,
                gen_id: body.gen_id,
                sigma: body.sigma,
                image: base64::engine::general_purpose::STANDARD
                    .decode(body.image.as_bytes())
                    .map_err(|source| invalid(raw.clone(), source))?,
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
    let chunk = ImageStreamChunk {
        event,
        raw: raw.into(),
    };
    if matches!(chunk.event, ImageStreamEvent::Error { .. }) {
        return Err(ImageStreamError::Provider {
            event: Box::new(chunk),
        }
        .into());
    }
    Ok(chunk)
}

pub(super) fn decode_sse(mut input: super::sse::SseStream) -> ImageStreamResult {
    Box::pin(async_stream::try_stream! {
        while let Some(event) = input.next().await {
            yield decode_event(event?)?;
        }
    })
}

#[cfg(test)]
mod tests;

pub(super) fn decode_msgpack(mut input: super::msgpack::MessagePackStream) -> ImageStreamResult {
    Box::pin(async_stream::try_stream! {
        while let Some(payload) = input.next().await {
            yield messagepack::decode_event(payload?)?;
        }
    })
}
