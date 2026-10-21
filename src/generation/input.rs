use serde::{Deserialize, Serialize};
use crate::Model;

/// Image-to-image and inpainting input.
///
/// Presence of [`Img2ImgRequest::mask`] switches the request from `img2img` to `infill`, which
/// also swaps the wire model to the inpainting sibling returned by [`Model::inpainting_id`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Img2ImgRequest {
    /// Validated source image.
    ///
    /// The client fits this image to the requested canvas before sending it.
    pub image: crate::EncodedImage,
    /// Strength value.
    ///
    /// Image-to-image requests accept `[0.01, 0.99]`; adding a mask selects the Inpaint profile,
    /// which accepts `[0.01, 1.0]`.
    pub strength: f32,
    /// Noise value in the `[0, 0.99]` range.
    pub noise: f32,
    /// Optional validated inpaint mask.
    pub mask: Option<crate::EncodedImage>,
    /// Whether the server applies color correction against the source image.
    pub color_correct: bool,
    /// Optional explicit `extra_noise_seed`. Defaults to the resolved request seed.
    pub extra_noise_seed: Option<i64>,
    /// Whether the server composites the untouched source outside the generated area.
    pub add_original_image: bool,
}

impl Img2ImgRequest {
    /// Creates an image-to-image request with the client defaults.
    #[must_use]
    pub const fn new(image: crate::EncodedImage, strength: f32, noise: f32) -> Self {
        Self {
            image,
            strength,
            noise,
            mask: None,
            color_correct: true,
            extra_noise_seed: None,
            // A live infill A/B confirms `true` leaves the area outside the mask pixel-identical
            // to the source, while `false` re-encodes the whole canvas and loses fidelity there.
            add_original_image: true,
        }
    }

    /// Returns `true` when this request targets the infill pipeline.
    #[must_use]
    pub const fn is_infill(&self) -> bool {
        self.mask.is_some()
    }
}
