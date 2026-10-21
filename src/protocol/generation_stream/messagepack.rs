use std::{fmt, io::Cursor};

use serde::{
    Deserialize,
    de::{DeserializeOwned, IgnoredAny, MapAccess, SeqAccess, Visitor},
};

use crate::{
    BridgeError, ImageDecodePolicy, ImageStreamChunk, ImageStreamError, ImageStreamEvent,
    StreamEventSource,
};

const MAX_NESTING: usize = 64;

struct Discriminator {
    event_type: String,
}

/// `MessagePack` strings and binary blobs have distinct protocol meanings. Serde's
/// standard String visitor also accepts UTF-8 bytes, so use a string-only visitor here.
struct StrictString(String);

impl<'de> Deserialize<'de> for StrictString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;
        impl Visitor<'_> for StringVisitor {
            type Value = StrictString;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a MessagePack string")
            }

            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictString(value.to_owned()))
            }

            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictString(value))
            }
        }
        deserializer.deserialize_string(StringVisitor)
    }
}

impl<'de> Deserialize<'de> for Discriminator {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct IdentityVisitor;
        impl<'de> Visitor<'de> for IdentityVisitor {
            type Value = Discriminator;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an event map with string keys")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut event_type = None;
                while let Some(StrictString(key)) = map.next_key::<StrictString>()? {
                    if key == "event_type" {
                        if event_type.is_some() {
                            return Err(serde::de::Error::duplicate_field("event_type"));
                        }
                        event_type = Some(map.next_value::<StrictString>()?.0);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(Discriminator {
                    event_type: event_type
                        .ok_or_else(|| serde::de::Error::missing_field("event_type"))?,
                })
            }
        }
        deserializer.deserialize_map(IdentityVisitor)
    }
}

#[derive(Deserialize)]
struct Intermediate {
    samp_ix: u32,
    step_ix: u32,
    gen_id: Option<u32>,
    sigma: Option<f32>,
    image: ImageBytes,
}

#[derive(Deserialize)]
struct Final {
    samp_ix: u32,
    gen_id: Option<u32>,
    image: ImageBytes,
}

#[derive(Deserialize)]
struct ProviderError {
    samp_ix: Option<u32>,
    gen_id: Option<u32>,
    message: StrictString,
}

struct ImageBytes(Vec<u8>);

impl<'de> Deserialize<'de> for ImageBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = ImageBytes;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("binary image bytes or an array of byte values")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
                Ok(ImageBytes(bytes.to_vec()))
            }

            fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Self::Value, E> {
                Ok(ImageBytes(bytes))
            }

            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<Self::Value, S::Error> {
                // Container headers are untrusted; never reserve their declared length.
                let mut bytes = Vec::new();
                while let Some(byte) = seq.next_element::<u8>()? {
                    bytes.push(byte);
                }
                Ok(ImageBytes(bytes))
            }
        }
        deserializer.deserialize_bytes(BytesVisitor)
    }
}

fn malformed(message: &'static str) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

/// Typed decoding avoids allocating unknown extension trees. Cursor-backed reads only copy
/// bytes actually present in the bounded frame, even for a malicious bin32/string length.
fn parse<T: DeserializeOwned>(
    payload: &[u8],
) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    if !matches!(payload.first(), Some(0x80..=0x8F | 0xDE | 0xDF)) {
        return Err(malformed("MessagePack event must be a map"));
    }
    let mut decoder = rmp_serde::Deserializer::new(Cursor::new(payload));
    decoder.set_max_depth(MAX_NESTING);
    let value = T::deserialize(&mut decoder)?;
    if decoder.position() != payload.len() as u64 {
        return Err(malformed("MessagePack frame contains trailing data"));
    }
    Ok(value)
}

pub(super) fn decode_event(payload: Vec<u8>) -> Result<ImageStreamChunk, BridgeError> {
    let event = decode_known(&payload).map_err(|source| ImageStreamError::InvalidEvent {
        raw: Box::new(StreamEventSource::MessagePack(payload.clone())),
        source,
    })?;
    let event = match event {
        ParsedEvent::Event(event) => event,
        ParsedEvent::Final(body) => {
            let image = crate::image::validated_generated_image(
                body.image.0,
                crate::GeneratedImageSource::Raw,
                ImageDecodePolicy::default(),
            )
            .map_err(|source| ImageStreamError::InvalidFinal {
                samp_ix: body.samp_ix,
                raw: Box::new(StreamEventSource::MessagePack(payload.clone())),
                source: Box::new(source),
            })?;
            ImageStreamEvent::Final {
                samp_ix: body.samp_ix,
                gen_id: body.gen_id,
                image,
            }
        },
    };
    let chunk = ImageStreamChunk {
        event,
        raw: StreamEventSource::MessagePack(payload),
    };
    if matches!(chunk.event, ImageStreamEvent::Error { .. }) {
        return Err(ImageStreamError::Provider {
            event: Box::new(chunk),
        }
        .into());
    }
    Ok(chunk)
}

enum ParsedEvent {
    Event(ImageStreamEvent),
    Final(Final),
}

fn decode_known(payload: &[u8]) -> Result<ParsedEvent, Box<dyn std::error::Error + Send + Sync>> {
    // Decode the identity first: unknown extensions may give familiar keys arbitrary types.
    let discriminator: Discriminator = parse(payload)?;
    Ok(match discriminator.event_type.as_str() {
        "intermediate" => {
            let body: Intermediate = parse(payload)?;
            ParsedEvent::Event(ImageStreamEvent::Intermediate {
                samp_ix: body.samp_ix,
                step_ix: body.step_ix,
                gen_id: body.gen_id,
                sigma: body.sigma,
                image: body.image.0,
            })
        },
        "final" => ParsedEvent::Final(parse(payload)?),
        "error" => {
            let body: ProviderError = parse(payload)?;
            ParsedEvent::Event(ImageStreamEvent::Error {
                samp_ix: body.samp_ix,
                gen_id: body.gen_id,
                message: body.message.0,
            })
        },
        _ => ParsedEvent::Event(ImageStreamEvent::Unknown {
            event_type: discriminator.event_type,
        }),
    })
}

#[cfg(test)]
mod tests;
