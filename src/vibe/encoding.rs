use std::sync::Arc;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use crate::{BridgeError, InvalidRequest};

#[cfg(test)]
mod tests;

/// Opaque vibe encoding bytes, distinct from an image payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VibeEncoding(Arc<[u8]>);

impl VibeEncoding {
    /// Creates a non-empty opaque encoding.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the encoding is empty.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(InvalidRequest::EmptyField {
                field: "vibe_encoding".to_owned(),
            }
            .into());
        }
        Ok(Self(bytes.into()))
    }
    /// Decodes canonical or whitespace-padded base64.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error for empty or malformed base64.
    pub fn from_base64(value: &str) -> Result<Self, BridgeError> {
        Self::from_bytes(crate::payload::decode_base64("vibe_encoding", value)?)
    }
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(&self.0)
    }
}
impl Serialize for VibeEncoding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base64())
    }
}
impl<'de> Deserialize<'de> for VibeEncoding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_base64(&value).map_err(D::Error::custom)
    }
}
