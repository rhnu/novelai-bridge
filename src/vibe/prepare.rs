use super::{EncodeVibeRequest, wire::VibeRequestBody};
use crate::{
    BridgeError, EncodeError, EncodeTarget, InvalidRequest, Model, RequestField,
    ValidationAdjustment, parameters::bounds::normalize_float,
};

/// A validated encode-vibe operation with its encoded source image.
pub struct PreparedVibe {
    model: Model,
    body: VibeRequestBody,
    adjustments: Vec<ValidationAdjustment>,
}

impl EncodeVibeRequest {
    /// Validates the model and normalizes extraction strength without network I/O.
    ///
    /// # Errors
    ///
    /// Returns unsupported-model or invalid numeric parameter errors.
    pub fn prepare(self) -> Result<PreparedVibe, BridgeError> {
        self.image.validate_policy(
            RequestField::Image,
            crate::media_policy::ImageDecodePolicy::default(),
        )?;
        if !self.model.descriptor().can_encode_vibe() {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: RequestField::EncodeVibe,
                required_model: "a model implementing encoded vibe input".to_owned(),
            }
            .into());
        }
        let information_extracted = normalize_float(
            RequestField::VibeInformationExtracted,
            self.information_extracted,
            0.01,
            1.0,
            self.policy,
        )?;
        let body = VibeRequestBody {
            image: self.image.to_base64(),
            information_extracted,
            model: self.model.as_str(),
        };
        Ok(PreparedVibe {
            model: self.model,
            body,
            adjustments: if information_extracted.to_bits() == self.information_extracted.to_bits()
            {
                Vec::new()
            } else {
                vec![ValidationAdjustment {
                    field: RequestField::VibeInformationExtracted,
                    from: f64::from(self.information_extracted),
                    to: f64::from(information_extracted),
                }]
            },
        })
    }
}

impl PreparedVibe {
    /// Effective extraction strength after validation.
    #[must_use]
    pub const fn information_extracted(&self) -> f32 { self.body.information_extracted }

    /// Numeric changes made under normalize policy.
    #[must_use]
    pub fn adjustments(&self) -> &[ValidationAdjustment] { &self.adjustments }

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

impl std::fmt::Debug for PreparedVibe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedVibe")
            .field("model", &self.model)
            .field("information_extracted", &self.information_extracted())
            .field("adjustments", &self.adjustments)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
