use crate::{BridgeError, EncodeError, EncodeTarget, InvalidRequest, Model};
use crate::generation::bounds::normalize_float;
use super::{EncodeVibeRequest, wire::VibeRequestBody};

/// A validated encode-vibe operation with its encoded source image.
#[derive(Debug)]
pub struct PreparedVibe {
    model: Model,
    body: VibeRequestBody,
}

impl EncodeVibeRequest {
    /// Validates the model and normalizes extraction strength without network I/O.
    ///
    /// # Errors
    ///
    /// Returns unsupported-model or invalid numeric parameter errors.
    pub fn prepare(self) -> Result<PreparedVibe, BridgeError> {
        if !self.model.descriptor().can_encode_vibe() {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: "encode_vibe".to_owned(),
                required_model: "a model implementing encoded vibe input".to_owned(),
            }.into());
        }
        let information_extracted = normalize_float(
            "information_extracted", self.information_extracted, 0.01, 1.0, self.strict_mode,
        )?;
        let body = VibeRequestBody {
            image: self.image.to_base64(), information_extracted, model: self.model.as_str(),
        };
        Ok(PreparedVibe { model: self.model, body })
    }
}

impl PreparedVibe {
    /// Model used to prepare the encoding request.
    #[must_use]
    pub const fn model(&self) -> Model { self.model }

    /// Encodes the prepared operation without repeating validation.
    ///
    /// # Errors
    ///
    /// Returns JSON request encoding failures.
    pub fn encode(&self) -> Result<Vec<u8>, BridgeError> {
        serde_json::to_vec(&self.body)
            .map_err(|source| EncodeError::new(EncodeTarget::JsonRequest, Box::new(source)).into())
    }
}

#[cfg(test)]
mod tests;
