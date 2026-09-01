use crate::{ImageSize, Model, Sampler};

#[cfg(feature = "generation")]
use crate::{BridgeError, GenerateImageRequest, GenerateImageStreamRequest};

/// Anlas cost per `/ai/encode-vibe` call.
pub const VIBE_ENCODE_COST: u32 = 2;
/// Anlas cost per character reference, per generated image.
const CHARACTER_REFERENCE_COST: u32 = 5;
/// Number of active vibe encodings included before the overage charge starts.
const FREE_VIBE_REFERENCE_COUNT: u32 = 4;
/// Opus free-generation pixel ceiling.
const OPUS_FREE_PIXELS: u64 = 1024 * 1024;
/// Opus free-generation step ceiling.
const OPUS_FREE_STEPS: u32 = 28;
/// Maximum number of images in the pricing formula's resolution table.
///
/// The formula documents an 8-image cap for very small canvases, but the current generation API
/// accepts only 1–4 in `n_samples`. The effective cap exposed by the estimator is therefore the
/// smaller of this table value and the model's request sample constraint.
const MAX_PRICING_SAMPLES: u32 = 8;
/// Highest per-image price accepted by the web client.
const MAX_PER_IMAGE_COST: u32 = 140;
/// Linear area term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_TERM: f64 = 2.951_823_174_884_865e-6;
/// Area-times-steps term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_STEP_TERM: f64 = 5.753_298_233_447_344e-7;

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
    /// Output canvas.
    pub size: ImageSize,
    /// Sampling step count.
    pub steps: u32,
    /// Sampler. NAI 3, 4, 4.5, 5's branch-A formula currently does not depend on it, but it is retained so
    /// the standalone input remains isomorphic to a generation request.
    pub sampler: Sampler,
    /// Whether SMEA is enabled.
    pub smea: bool,
    /// Whether dynamic SMEA is enabled.
    pub smea_dyn: bool,
    /// Number of images requested. Values outside the API's `[1, 4]` range are clamped for the
    /// calculation and preserved as [`AnlasEstimate::requested_samples`].
    pub n_samples: u32,
    /// Image-to-image strength, when the request has an image source and no mask.
    pub img2img_strength: Option<f32>,
    /// `inpaintImg2ImgStrength`, when the request has an inpaint mask. `None` means the web client
    /// falls back to `1.0`.
    pub inpaint_img2img_strength: Option<f32>,
    /// Whether the request carries an inpaint mask.
    pub has_inpaint_mask: bool,
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
            sampler: defaults.sampling.sampler,
            smea: false,
            smea_dyn: false,
            n_samples: defaults.n_samples,
            img2img_strength: None,
            inpaint_img2img_strength: None,
            has_inpaint_mask: false,
            character_reference_count: 0,
            vibe_reference_count: 0,
            pending_vibe_encodes: 0,
            context: AnlasPricingContext::default(),
        }
    }
}

#[cfg(feature = "generation")]
impl AnlasEstimateInput {
    /// Normalizes a request and projects its effective pricing fields without decoding images.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors.
    pub fn for_request(
        request: &GenerateImageRequest,
        context: AnlasPricingContext,
    ) -> Result<Self, BridgeError> {
        request.anlas_estimate_input(context)
    }
}

#[cfg(feature = "generation")]
impl crate::NormalizedGeneration {
    /// Projects effective pricing fields without cloning image payloads or assembling prompts.
    #[must_use]
    pub fn anlas_estimate_input(&self, context: AnlasPricingContext) -> AnlasEstimateInput {
        let request = self.request();
        let references = request.references.as_ref();
        let has_inpaint_mask = request
            .img2img
            .as_ref()
            .is_some_and(crate::Img2ImgRequest::is_infill);
        AnlasEstimateInput {
            model: request.core.model,
            size: request.core.size,
            steps: request.sampling.steps,
            sampler: request.sampling.sampler,
            smea: request.guidance.smea,
            smea_dyn: request.guidance.smea_dyn,
            n_samples: request.core.n_samples,
            img2img_strength: request
                .img2img
                .as_ref()
                .filter(|_| !has_inpaint_mask)
                .map(|value| value.strength),
            inpaint_img2img_strength: request
                .img2img
                .as_ref()
                .filter(|_| has_inpaint_mask)
                .map(|value| value.strength),
            has_inpaint_mask,
            character_reference_count: references.map_or(0, |set| {
                u32::try_from(set.character_references.len()).unwrap_or(u32::MAX)
            }),
            vibe_reference_count: references
                .and_then(|set| set.vibe_transfer.as_ref())
                .map_or(0, |vibe| {
                    u32::try_from(vibe.references.len()).unwrap_or(u32::MAX)
                }),
            pending_vibe_encodes: 0,
            context,
        }
    }

    /// Estimates the effective request using an explicit account-context snapshot.
    #[must_use]
    pub fn estimate_anlas_cost(&self, context: AnlasPricingContext) -> AnlasEstimate {
        estimate_anlas_cost(self.anlas_estimate_input(context))
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

/// Estimates the Anlas cost of a NAI 3, 4, 4.5, 5 generation request.
///
/// The formula mirrors the cost function used by the `NovelAI` web client at the pricing-document
/// revision bundled with this crate. It intentionally does not implement V1/V2 or unrelated
/// models. The returned breakdown still contains the mathematical cost when the per-image price
/// exceeds 140; check [`AnlasEstimate::is_too_expensive`] to handle the web client's `-3` state.
#[must_use]
pub fn estimate_anlas_cost(input: AnlasEstimateInput) -> AnlasEstimate {
    let pixels = input.size.pixels();
    #[allow(clippy::cast_precision_loss)]
    let pixels_f = pixels as f64;
    // Keep the web client's operation order; an FMA can cross an integer ceil boundary.
    #[allow(clippy::suboptimal_flops)]
    let base = (COST_AREA_TERM * pixels_f
        + COST_AREA_STEP_TERM * pixels_f * f64::from(input.steps))
    .ceil();
    let smea_multiplier = if input.smea && input.smea_dyn {
        1.4
    } else if input.smea {
        1.2
    } else {
        1.0
    };
    let descriptor = input.model.descriptor();
    let base_cost = base * smea_multiplier * descriptor.pricing().multiplier;
    let strength = effective_strength(&input);
    let per_image = saturating_u32((base_cost * strength).ceil()).max(2);
    let status = if per_image > MAX_PER_IMAGE_COST {
        AnlasEstimateStatus::TooExpensive
    } else {
        AnlasEstimateStatus::Available
    };

    let requested_samples = input.n_samples;
    let sample_range = descriptor.constraints().samples;
    let api_samples = requested_samples.clamp(sample_range.min, sample_range.max);
    let sample_limit = pricing_sample_limit(pixels).min(sample_range.max);
    let priced_samples = api_samples.min(sample_limit);
    let character_reference_count = chargeable_character_reference_count(&input);
    let character_reference = character_reference_count
        .saturating_mul(CHARACTER_REFERENCE_COST)
        .saturating_mul(priced_samples);
    let vibe_encoding = if descriptor.can_encode_vibe() {
        input.pending_vibe_encodes.saturating_mul(VIBE_ENCODE_COST)
    } else {
        0
    };
    let vibe_reference_overage = if descriptor.can_encode_vibe() {
        input
            .vibe_reference_count
            .saturating_sub(FREE_VIBE_REFERENCE_COUNT)
            .saturating_mul(VIBE_ENCODE_COST)
    } else {
        0
    };
    let free_first_image_applied =
        is_free_first_image_eligible(&input, pixels, character_reference_count, priced_samples);
    let billable_samples = priced_samples.saturating_sub(u32::from(free_first_image_applied));
    let generation = per_image.saturating_mul(billable_samples);
    let total = generation
        .saturating_add(character_reference)
        .saturating_add(vibe_encoding)
        .saturating_add(vibe_reference_overage);

    AnlasEstimate {
        status,
        per_image,
        generation,
        character_reference,
        vibe_encoding,
        vibe_reference_overage,
        total,
        requested_samples,
        sample_limit,
        priced_samples,
        billable_samples,
        free_first_image_applied,
    }
}

#[cfg(feature = "generation")]
impl GenerateImageRequest {
    /// Normalizes and returns effective pricing fields.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors.
    pub fn anlas_estimate_input(
        &self,
        context: AnlasPricingContext,
    ) -> Result<AnlasEstimateInput, BridgeError> {
        Ok(self.clone().normalize()?.anlas_estimate_input(context))
    }

    /// Estimates the effective request without network I/O or image pixel decoding.
    ///
    /// Unlike the independent formula, this validates the request's shape and policy.
    /// Reuse a normalized plan to avoid cloning payloads on repeated estimates.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors.
    pub fn estimate_anlas_cost(
        &self,
        context: AnlasPricingContext,
    ) -> Result<AnlasEstimate, BridgeError> {
        Ok(estimate_anlas_cost(self.anlas_estimate_input(context)?))
    }
}

#[cfg(feature = "generation")]
impl GenerateImageStreamRequest {
    /// Normalizes and estimates the base request.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors. Streaming support is checked when sending.
    pub fn estimate_anlas_cost(
        &self,
        context: AnlasPricingContext,
    ) -> Result<AnlasEstimate, BridgeError> {
        self.base.estimate_anlas_cost(context)
    }
}

/// Estimates the cost of a director tool or upscale call.
///
/// `NovelAI` charges these from a separate pixel-bucket table; Opus covers inputs up to 409600
/// pixels. This helper is independent from the NAI 3, 4, 4.5, 5 image-generation formula.
#[must_use]
pub const fn estimate_director_tool_cost(size: ImageSize, context: AnlasPricingContext) -> u32 {
    let pixels = size.pixels();
    if context.tier >= 3 && context.subscription_active && pixels <= 409_600 {
        return 0;
    }
    if pixels <= 262_144 {
        1
    } else if pixels <= 409_600 {
        2
    } else if pixels <= 524_288 {
        3
    } else if pixels <= 786_432 {
        5
    } else {
        7
    }
}

const fn chargeable_character_reference_count(input: &AnlasEstimateInput) -> u32 {
    let descriptor = input.model.descriptor();
    let mode = if input.has_inpaint_mask {
        crate::GenerationMode::Inpainting
    } else if input.img2img_strength.is_some() {
        crate::GenerationMode::ImageToImage
    } else {
        crate::GenerationMode::TextToImage
    };
    let references_allowed = descriptor
        .resolve_references(
            mode,
            crate::ReferenceIntent {
                vibe: false,
                character: true,
            },
        )
        .character
        .is_available();
    if references_allowed {
        input.character_reference_count
    } else {
        0
    }
}

const fn is_free_first_image_eligible(
    input: &AnlasEstimateInput,
    pixels: u64,
    character_reference_count: u32,
    capped_samples: u32,
) -> bool {
    let descriptor = input.model.descriptor();
    input.context.tier >= 3
        && input.context.subscription_active
        && !input.context.free_first_image_used
        && character_reference_count == 0
        && pixels <= OPUS_FREE_PIXELS
        && input.steps <= OPUS_FREE_STEPS
        && !(matches!(
            descriptor.pricing().opus_allowance,
            crate::OpusAllowance::Metered
        ) && input.context.v5_usage_is_negative)
        && capped_samples > 0
}

/// Returns the pricing formula's resolution-dependent image-count cap before the request API cap.
const fn pricing_sample_limit(pixels: u64) -> u32 {
    if pixels <= 360_448 {
        MAX_PRICING_SAMPLES
    } else if pixels <= 409_600 {
        6
    } else if pixels <= 3_145_728 {
        4
    } else {
        0
    }
}

fn effective_strength(input: &AnlasEstimateInput) -> f64 {
    let value = if input.has_inpaint_mask {
        input.inpaint_img2img_strength.unwrap_or(1.0)
    } else {
        input.img2img_strength.unwrap_or(1.0)
    };
    let value = f64::from(value);
    if value.is_finite() { value } else { 1.0 }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn saturating_u32(value: f64) -> u32 {
    if value <= 0.0 {
        0
    } else if value >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        value as u32
    }
}
