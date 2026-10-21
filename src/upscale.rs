//! Standalone upscaling requests, preparation and local cost estimates.
mod prepare;

pub use prepare::PreparedUpscale;
use serde::{Deserialize, Serialize};

use crate::{EncodedImage, GeneratedImage, ImageSize, Model};

/// Declared input blur supported by the standalone upscaler.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum UpscaleBlurSigma {
    #[default]
    None,
    Sigma030,
    Sigma035,
    Sigma040,
    Sigma045,
    Sigma050,
}

impl UpscaleBlurSigma {
    /// Numeric blur declaration sent to `NovelAI`.
    #[must_use]
    pub const fn sigma(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Sigma030 => 0.30,
            Self::Sigma035 => 0.35,
            Self::Sigma040 => 0.40,
            Self::Sigma045 => 0.45,
            Self::Sigma050 => 0.50,
        }
    }
}

impl Serialize for UpscaleBlurSigma {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_f64(self.sigma())
    }
}

impl<'de> Deserialize<'de> for UpscaleBlurSigma {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let sigma = f64::deserialize(deserializer)?;
        let variants = [
            Self::None,
            Self::Sigma030,
            Self::Sigma035,
            Self::Sigma040,
            Self::Sigma045,
            Self::Sigma050,
        ];
        variants
            .into_iter()
            .find(|value| value.sigma().to_bits() == sigma.to_bits())
            .ok_or_else(|| {
                serde::de::Error::custom("blur sigma must be 0, 0.30, 0.35, 0.40, 0.45 or 0.50")
            })
    }
}

/// Input to `NovelAI`'s standalone upscaler, independent from generation and Director Tools.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpscaleImageRequest {
    /// Validated source image, sent at its original dimensions.
    pub image: EncodedImage,
    /// Existing image-model identity used by the upscaler.
    #[serde(default = "default_model")]
    pub model: Model,
    /// Declared source blur; this does not apply a local blur transformation.
    #[serde(default)]
    pub declared_blur_sigma: UpscaleBlurSigma,
}

impl UpscaleImageRequest {
    /// Uses the frontend's current V5 Curated model and zero-blur defaults.
    #[must_use]
    pub const fn new(image: EncodedImage) -> Self {
        Self {
            image,
            model: default_model(),
            declared_blur_sigma: UpscaleBlurSigma::None,
        }
    }
}

const fn default_model() -> Model { Model::NaiDiffusion5Curated }

/// Every validated image returned by a standalone upscaling request.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct UpscaleImageResult {
    pub images: Vec<GeneratedImage>,
}

/// Estimates the standalone upscaler's current input-area price table.
///
/// No subscription discount applies. An unknown area returns `None`; that is not a
/// server-side rejection or a zero-cost estimate.
#[must_use]
pub const fn estimate_upscale_cost(size: ImageSize) -> Option<u32> {
    if size.width == 0 || size.height == 0 {
        return None;
    }
    match size.pixels() {
        0..=1_048_576 => Some(1),
        1_048_577..=1_747_627 => Some(2),
        1_747_628..=2_446_678 => Some(3),
        2_446_679..=3_145_728 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
