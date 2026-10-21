use crate::{ImageSize, Model};

mod auxiliary;
mod formula;

pub use auxiliary::{VIBE_ENCODE_COST, estimate_director_tool_cost};
pub use formula::estimate_anlas_cost;

/// Cost-bearing generation mode; only the selected operation carries a strength.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum PricingMode {
    /// Generation without a source image.
    #[default]
    TextToImage,
    /// Generation from a source image.
    ImageToImage { strength: f32 },
    /// Generation within a mask.
    Inpainting { strength: f32 },
}

impl PricingMode {
    pub(crate) const fn generation_mode(self) -> crate::GenerationMode {
        match self {
            Self::TextToImage => crate::GenerationMode::TextToImage,
            Self::ImageToImage { .. } => crate::GenerationMode::ImageToImage,
            Self::Inpainting { .. } => crate::GenerationMode::Inpainting,
        }
    }

    pub(crate) fn effective_strength(self) -> f64 {
        let value = match self {
            Self::TextToImage => 1.0,
            Self::ImageToImage { strength } | Self::Inpainting { strength } => f64::from(strength),
        };
        if value.is_finite() { value } else { 1.0 }
    }
}

/// Account and call state that affects Anlas pricing.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct AnlasPricingContext {
    /// Numeric subscription tier.
    pub tier: i32,
    /// Whether the paid subscription is currently active.
    pub subscription_active: bool,
    /// Whether the free V5 usage allowance is overdrawn.
    pub v5_usage_is_negative: bool,
    /// Whether another part of the application already consumed the free first-image allowance.
    pub free_first_image_used: bool,
}

/// Result state for a local Anlas estimate.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AnlasEstimateStatus {
    /// The per-image price is within the web client's accepted limit.
    Available,
    /// The web client would reject this request because its per-image price exceeds 140 Anlas.
    TooExpensive,
}

/// Inputs to [`estimate_anlas_cost`].
///
/// This input intentionally contains the same cost-bearing fields as a
/// `GenerateImageRequest`. Use `GenerateImageRequest::anlas_estimate_input` when a request is
/// already assembled; the free calculator and the request/client helpers then share exactly the
/// same formula.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AnlasEstimateInput {
    /// Target NAI 3, NAI 4, NAI 4.5 or NAI 5 model.
    pub model: Model,
    /// Effective pricing canvas; use the normalized request projection for Max Enhance.
    pub size: ImageSize,
    /// Sampling step count.
    pub steps: u32,
    /// Whether SMEA is enabled.
    pub smea: bool,
    /// Whether dynamic SMEA is enabled.
    pub smea_dyn: bool,
    /// Number of images requested. Values outside the API's `[1, 8]` range are clamped for the
    /// calculation and preserved as [`AnlasEstimate::requested_samples`].
    pub n_samples: u32,
    /// The selected operation and its effective strength.
    pub mode: PricingMode,
    /// Number of character references in the request.
    pub character_reference_count: u32,
    /// Number of active encoded vibe references.
    pub vibe_reference_count: u32,
    /// Number of vibe encodings that still need an `/ai/encode-vibe` call.
    pub pending_vibe_encodes: u32,
    /// Subscription and allowance state used for the Opus first-image rule.
    pub context: AnlasPricingContext,
}

impl Default for AnlasEstimateInput {
    fn default() -> Self {
        let model = Model::default();
        let defaults = model.descriptor().defaults();
        Self {
            model,
            size: defaults.size,
            steps: defaults.sampling.steps,
            smea: false,
            smea_dyn: false,
            n_samples: defaults.n_samples,
            mode: PricingMode::TextToImage,
            character_reference_count: 0,
            vibe_reference_count: 0,
            pending_vibe_encodes: 0,
            context: AnlasPricingContext::default(),
        }
    }
}

/// Breakdown of an estimated Anlas cost.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AnlasEstimate {
    /// Whether the per-image price can be accepted by the web client.
    pub status: AnlasEstimateStatus,
    /// Cost of a single image before the Opus free allowance is applied.
    pub per_image: u32,
    /// Generation cost after the Opus free allowance is applied.
    pub generation: u32,
    /// Character reference surcharge.
    pub character_reference: u32,
    /// One-off vibe encoding cost for encodings that are not already cached/registered.
    pub vibe_encoding: u32,
    /// Additional surcharge for active vibe references beyond the first four.
    pub vibe_reference_overage: u32,
    /// Sum of all components.
    pub total: u32,
    /// Number of images requested before any caps.
    pub requested_samples: u32,
    /// Maximum number of images allowed for the output pixel count.
    pub sample_limit: u32,
    /// Number of images retained after the API and resolution caps.
    pub priced_samples: u32,
    /// Number of images remaining after the resolution cap and Opus first-image allowance.
    pub billable_samples: u32,
    /// Whether the Opus first-image allowance was applied.
    pub free_first_image_applied: bool,
}

impl AnlasEstimate {
    /// Returns `true` when the web client would reject the per-image price.
    #[must_use]
    pub const fn is_too_expensive(&self) -> bool {
        matches!(self.status, AnlasEstimateStatus::TooExpensive)
    }

    /// Returns `true` when the request is available and its estimated total is zero.
    #[must_use]
    pub const fn is_free(&self) -> bool {
        matches!(self.status, AnlasEstimateStatus::Available)
            && self.priced_samples > 0
            && self.total == 0
    }
}

#[cfg(test)]
mod tests;
