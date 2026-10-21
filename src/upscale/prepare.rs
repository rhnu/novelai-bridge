use serde::Serialize;

use super::{UpscaleBlurSigma, UpscaleImageRequest, estimate_upscale_cost};
use crate::{
    BridgeError, EncodeError, EncodeTarget, ImageDecodePolicy, ImageSize, Model, RequestField,
};

#[derive(Serialize)]
struct UpscaleRequestBody {
    image: crate::image::request_body::Media,
    model: Model,
    declared_blur_sigma: UpscaleBlurSigma,
}

/// Validated upscaling input retaining the source dimensions and reusable wire payload.
pub struct PreparedUpscale {
    image_size: ImageSize,
    body: UpscaleRequestBody,
}

impl UpscaleImageRequest {
    /// Rechecks default media budgets without resizing or decoding the image again.
    ///
    /// # Errors
    /// Returns an image validation or resource-budget error.
    pub fn prepare(self) -> Result<PreparedUpscale, BridgeError> {
        self.image
            .validate_policy(RequestField::Image, ImageDecodePolicy::default())?;
        Ok(PreparedUpscale {
            image_size: self.image.size(),
            body: UpscaleRequestBody {
                image: crate::image::request_body::Media::Image(self.image.into_shared_bytes()),
                model: self.model,
                declared_blur_sigma: self.declared_blur_sigma,
            },
        })
    }
}

impl PreparedUpscale {
    pub(crate) fn take_request_body(
        &mut self,
    ) -> Result<crate::image::request_body::RequestBody, BridgeError> {
        let mut media = Vec::new();
        self.body.image.extract("/image", &mut media);
        crate::image::request_body::RequestBody::new(&self.body, media)
    }

    /// Original image dimensions used by preparation and local pricing.
    #[must_use]
    pub const fn image_size(&self) -> ImageSize { self.image_size }

    /// Model identity sent to the service.
    #[must_use]
    pub const fn model(&self) -> Model { self.body.model }

    /// Source blur declaration sent to the service.
    #[must_use]
    pub const fn declared_blur_sigma(&self) -> UpscaleBlurSigma { self.body.declared_blur_sigma }

    /// Current frontend price estimate, or no estimate beyond its known table.
    #[must_use]
    pub const fn estimated_cost(&self) -> Option<u32> { estimate_upscale_cost(self.image_size) }

    /// Encodes the prepared payload without repeating image validation.
    ///
    /// # Errors
    /// Returns JSON request encoding failures.
    pub fn encode(&self) -> Result<Vec<u8>, BridgeError> {
        serde_json::to_vec(&self.body)
            .map_err(|source| EncodeError::new(EncodeTarget::JsonRequest, Box::new(source)).into())
    }
}

impl std::fmt::Debug for PreparedUpscale {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedUpscale")
            .field("model", &self.body.model)
            .field("image_size", &self.image_size)
            .field("declared_blur_sigma", &self.body.declared_blur_sigma)
            .finish_non_exhaustive()
    }
}
