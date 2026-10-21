use std::{fmt, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::DynamicImage;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{BridgeError, ImageSize, InvalidRequest};

/// Validated image input with raw bytes and decoded pixels cached together.
#[derive(Clone)]
pub struct EncodedImage {
    bytes: Arc<[u8]>,
    decoded: Arc<DynamicImage>,
}

impl EncodedImage {
    /// Validates image bytes and caches their decoded pixels.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the bytes are not a decodable image.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        let decoded = decode_input_image("image", &bytes)?;
        Ok(Self {
            bytes: bytes.into(),
            decoded: Arc::new(decoded),
        })
    }

    /// Decodes a base64 image or image data URL, validates it, and caches its pixels.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error for malformed base64, data URLs or image bytes.
    pub fn from_base64(value: &str) -> Result<Self, BridgeError> {
        Self::from_bytes(crate::payload::decode_image("image", value)?)
    }

    /// Original validated bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] { &self.bytes }

    /// Canonical base64 representation used by wire payloads.
    #[must_use]
    pub fn to_base64(&self) -> String { STANDARD.encode(&self.bytes) }

    /// Decoded dimensions.
    #[must_use]
    pub fn size(&self) -> ImageSize {
        ImageSize {
            width: self.decoded.width(),
            height: self.decoded.height(),
        }
    }

    /// Cached decoded pixels, borrowed without re-decoding or allowing mutation.
    #[must_use]
    pub fn pixels(&self) -> &DynamicImage { &self.decoded }
}

impl fmt::Debug for EncodedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedImage")
            .field("size", &self.size())
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for EncodedImage {
    fn eq(&self, other: &Self) -> bool { self.bytes == other.bytes }
}

impl Eq for EncodedImage {}

impl Serialize for EncodedImage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base64())
    }
}

impl<'de> Deserialize<'de> for EncodedImage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_base64(&value).map_err(D::Error::custom)
    }
}

fn decode_input_image(field: &str, bytes: &[u8]) -> Result<DynamicImage, BridgeError> {
    image::load_from_memory(bytes).map_err(|source| {
        InvalidRequest::UndecodableImage {
            field: field.to_owned(),
            source: Box::new(source),
        }
        .into()
    })
}
