use serde::{Deserialize, Serialize};

use crate::{EncodedImage, GenerationMode};

/// Generation operation and the input fields meaningful for that operation.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "mode",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum GenerationInput {
    /// Generation without an image source.
    #[default]
    TextToImage,
    /// Generation from a source image.
    ImageToImage(ImageToImageInput),
    /// Generation restricted to a required mask.
    Inpainting(InpaintingInput),
}

impl GenerationInput {
    /// Operation selected by this input, independent of delivery and model identity.
    #[must_use]
    pub const fn mode(&self) -> GenerationMode {
        match self {
            Self::TextToImage => GenerationMode::TextToImage,
            Self::ImageToImage(_) => GenerationMode::ImageToImage,
            Self::Inpainting(_) => GenerationMode::Inpainting,
        }
    }
}

/// Image-to-image input. Its options cannot be silently reused by inpainting.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ImageToImageInput {
    /// Validated source image, fitted to the generation canvas during preparation.
    pub image: EncodedImage,
    /// Source influence in `[0.01, 0.99]`.
    pub strength: f32,
    /// Source noise in `[0, 0.99]`.
    pub noise: f32,
    /// Whether the server applies source color correction.
    pub color_correct: bool,
    /// Explicit noise seed, otherwise the resolved request seed.
    pub extra_noise_seed: Option<i64>,
    /// Whether the server retains the original source image.
    pub add_original_image: bool,
}

impl ImageToImageInput {
    /// Creates source-image input with the provider defaults.
    #[must_use]
    pub const fn new(image: EncodedImage, strength: f32, noise: f32) -> Self {
        Self {
            image,
            strength,
            noise,
            color_correct: true,
            extra_noise_seed: None,
            add_original_image: true,
        }
    }
}

/// Inpainting input with a required mask and its own strength range.
///
/// ```compile_fail
/// use novelai_bridge::InpaintingInput;
/// fn invalid(input: &mut InpaintingInput) {
///     input.extra_noise_seed = Some(42);
/// }
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InpaintingInput {
    /// Validated source image, fitted to the generation canvas.
    pub image: EncodedImage,
    /// Required validated mask, fitted using nearest-neighbor sampling.
    pub mask: EncodedImage,
    /// Masked-generation strength in `[0.01, 1.0]`.
    pub strength: f32,
    /// Source noise in `[0, 0.99]`.
    pub noise: f32,
    /// Preserve untouched pixels outside the mask rather than re-encoding the whole canvas.
    pub add_original_image: bool,
}

impl InpaintingInput {
    /// Creates masked input that preserves the untouched source outside the mask.
    #[must_use]
    pub const fn new(image: EncodedImage, mask: EncodedImage, strength: f32, noise: f32) -> Self {
        Self {
            image,
            mask,
            strength,
            noise,
            add_original_image: true,
        }
    }
}

#[cfg(test)]
mod tests;
