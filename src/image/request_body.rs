//! Media stays binary until the final transport encoding is selected.
use base64::{display::Base64Display, engine::general_purpose::STANDARD};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;

use crate::{BridgeError, PreparedImage, VibeEncoding};

#[derive(Default)]
pub enum Media {
    #[default]
    Empty,
    Prepared(PreparedImage),
    Image(std::sync::Arc<[u8]>),
    Vibe(VibeEncoding),
}

impl Media {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Prepared(image) => image.as_bytes(),
            Self::Image(bytes) => bytes,
            Self::Vibe(encoding) => encoding.as_bytes(),
        }
    }

    pub(crate) fn extract(&mut self, pointer: impl Into<String>, media: &mut Vec<MediaSlot>) {
        media.push(MediaSlot {
            pointer: pointer.into(),
            data: std::mem::take(self),
            omitted: false,
        });
    }
}

impl Serialize for Media {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&Base64Display::new(self.bytes(), &STANDARD))
    }
}

pub struct MediaSlot {
    pub(crate) pointer: String,
    pub(crate) data: Media,
    pub(crate) omitted: bool,
}

pub struct MediaBody {
    pub(crate) value: Value,
    pub(crate) media: Vec<MediaSlot>,
}

pub enum RequestBody {
    Json(Vec<u8>),
    Media(MediaBody),
}

impl RequestBody {
    pub(crate) fn new(value: &impl Serialize, media: Vec<MediaSlot>) -> Result<Self, BridgeError> {
        MediaBody::new(value, media).map(Self::Media)
    }
}

impl MediaBody {
    pub(crate) fn new(value: &impl Serialize, media: Vec<MediaSlot>) -> Result<Self, BridgeError> {
        // All media has already been extracted. Round-trip only the small metadata tree so
        // f32 fields retain the exact JSON numbers produced by public prepared.encode().
        let json = serde_json::to_vec(value).map_err(encode_error)?;
        Ok(Self {
            value: serde_json::from_slice(&json).map_err(encode_error)?,
            media,
        })
    }

    pub(crate) fn encode_json(&self) -> Result<Vec<u8>, BridgeError> {
        encode_projection(&self.value, self.media.iter())
    }
}

/// Serializes the small semantic tree while streaming each active binary field as base64.
struct JsonValue<'a> {
    value: &'a Value,
    media: &'a std::collections::HashMap<&'a str, &'a Media>,
    pointer: String,
}

impl Serialize for JsonValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(media) = self.media.get(self.pointer.as_str()) {
            return media.serialize(serializer);
        }
        match self.value {
            Value::Object(object) => {
                let mut map = serializer.serialize_map(Some(object.len()))?;
                for (key, value) in object {
                    map.serialize_entry(
                        key,
                        &Self {
                            value,
                            media: self.media,
                            pointer: format!("{}/{key}", self.pointer),
                        },
                    )?;
                }
                map.end()
            },
            Value::Array(array) => {
                let mut sequence = serializer.serialize_seq(Some(array.len()))?;
                for (index, value) in array.iter().enumerate() {
                    sequence.serialize_element(&Self {
                        value,
                        media: self.media,
                        pointer: format!("{}/{index}", self.pointer),
                    })?;
                }
                sequence.end()
            },
            value => value.serialize(serializer),
        }
    }
}

pub fn encode_error(source: serde_json::Error) -> BridgeError {
    crate::EncodeError::new(crate::EncodeTarget::JsonRequest, Box::new(source)).into()
}

pub fn encode_projection<'a>(
    value: &Value,
    media: impl Iterator<Item = &'a MediaSlot>,
) -> Result<Vec<u8>, BridgeError> {
    let media: std::collections::HashMap<_, _> = media
        .filter(|media| !media.omitted)
        .map(|media| (media.pointer.as_str(), &media.data))
        .collect();
    if media.is_empty() {
        return serde_json::to_vec(value).map_err(encode_error);
    }
    serde_json::to_vec(&JsonValue {
        value,
        media: &media,
        pointer: String::new(),
    })
    .map_err(encode_error)
}
