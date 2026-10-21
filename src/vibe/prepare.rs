use super::{EncodeVibeRequest, wire::VibeRequestBody};
use crate::{
    BridgeError, EncodeError, EncodeTarget, ImagePreparation, ImagePreprocessor, ImageSize,
    InvalidRequest, Model, RequestField, ValidationAdjustment, parameters::bounds::normalize_float,
};

/// A validated encode-vibe operation with its encoded source image.
pub struct PreparedVibe {
    model: Model,
    image_size: ImageSize,
    body: VibeRequestBody,
    adjustments: Vec<ValidationAdjustment>,
}

impl EncodeVibeRequest {
    /// Validates extraction options and prepares an optional source mask without network I/O.
    ///
    /// # Errors
    ///
    /// Returns model, extraction, crop-context or image-preparation failures.
    pub fn prepare(self) -> Result<PreparedVibe, BridgeError> { self.prepare_inner(None) }

    /// Prepares an optional mask with a caller-provided pixel transformer.
    ///
    /// # Errors
    /// Returns model, extraction, crop-context or image-preparation failures.
    pub fn prepare_with(
        self,
        preprocessor: &dyn ImagePreprocessor,
    ) -> Result<PreparedVibe, BridgeError> {
        self.prepare_inner(Some(preprocessor))
    }

    fn prepare_inner(
        self,
        preprocessor: Option<&dyn ImagePreprocessor>,
    ) -> Result<PreparedVibe, BridgeError> {
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
            0.0,
            1.0,
            self.policy,
        )?;
        if self.crop_to_mask == Some(true) && self.mask.is_none() {
            return Err(InvalidRequest::RequiredFieldForContext {
                context: "crop_to_mask".to_owned(),
                field: RequestField::VibeMask,
            }
            .into());
        }
        let image_size = self.image.size();
        let mask = self
            .mask
            .as_ref()
            .map(|mask| {
                crate::image::prepare_image(
                    preprocessor,
                    RequestField::VibeMask,
                    mask,
                    ImagePreparation::Mask(image_size),
                )
                .map(crate::image::request_body::Media::Prepared)
            })
            .transpose()?;
        let body = VibeRequestBody {
            image: crate::image::request_body::Media::Image(self.image.into_shared_bytes()),
            information_extracted,
            model: self.model.as_str(),
            mask,
            crop_to_mask: self.crop_to_mask,
            focus_seed: self.focus_seed,
            info_extract_seed: self.info_extract_seed,
        };
        Ok(PreparedVibe {
            model: self.model,
            image_size,
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
    pub(crate) fn take_request_body(
        &mut self,
    ) -> Result<crate::image::request_body::RequestBody, BridgeError> {
        let mut media = Vec::new();
        self.body.image.extract("/image", &mut media);
        if let Some(mask) = &mut self.body.mask {
            mask.extract("/mask", &mut media);
        }
        crate::image::request_body::RequestBody::new(&self.body, media)
    }

    /// Effective extraction strength after validation.
    #[must_use]
    pub const fn information_extracted(&self) -> f32 { self.body.information_extracted }

    /// Numeric changes made under normalize policy.
    #[must_use]
    pub fn adjustments(&self) -> &[ValidationAdjustment] { &self.adjustments }

    /// Model used to prepare the encoding request.
    #[must_use]
    pub const fn model(&self) -> Model { self.model }

    /// Source dimensions shared by the original image and its prepared mask.
    #[must_use]
    pub const fn image_size(&self) -> ImageSize { self.image_size }

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
            .field("image_size", &self.image_size)
            .field("has_mask", &self.body.mask.is_some())
            .field("information_extracted", &self.information_extracted())
            .field("adjustments", &self.adjustments)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
