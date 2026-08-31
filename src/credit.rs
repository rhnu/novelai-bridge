//! Optional observation of request cost and account credit.
//!
//! A [`CreditObserver`] receives de-identified events for every request the client sends. The
//! crate never persists anything itself: it reports what was requested, what it estimates the
//! request costs, and what the account balance looked like when it was last sampled. Callers
//! combine those to build usage logs or credit models.

use async_trait::async_trait;

use crate::{
    BridgeError, GenerateImageRequest, GenerateImageStreamRequest, ImageSize, Model, Sampler,
    SubscriptionInfo, V5UsageStatus,
};

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
/// Maximum number of images accepted by the generation request API.
const MAX_REQUEST_SAMPLES: u32 = 4;
/// Maximum number of images in the pricing formula's resolution table.
///
/// The formula documents an 8-image cap for very small canvases, but the current generation API
/// accepts only 1–4 in `n_samples`. The effective cap exposed by the estimator is therefore the
/// smaller of this table value and [`MAX_REQUEST_SAMPLES`].
const MAX_PRICING_SAMPLES: u32 = 8;
/// Highest per-image price accepted by the web client.
const MAX_PER_IMAGE_COST: u32 = 140;
/// Linear area term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_TERM: f64 = 2.951_823_174_884_865e-6;
/// Area-times-steps term of the `NovelAI` NAI 3, 4, 4.5, 5 cost function.
const COST_AREA_STEP_TERM: f64 = 5.753_298_233_447_344e-7;

/// `NovelAI` operation an observed request belongs to.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObservedOperation {
    /// `/ai/generate-image`
    GenerateImage,
    /// `/ai/generate-image-stream`
    GenerateImageStream,
    /// `/ai/encode-vibe`
    EncodeVibe,
    /// `/ai/augment-image`
    AugmentImage,
    /// `/user/subscription`
    Subscription,
}

/// De-identified description of an observed request.
///
/// Prompt text, image payloads and credentials are never included; only shapes and sizes that
/// affect cost.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct RequestFingerprint {
    /// Process-local identifier pairing [`CreditEvent::RequestPlanned`] with
    /// [`CreditEvent::RequestSettled`].
    pub request_id: u64,
    /// Operation being performed.
    pub operation: ObservedOperation,
    /// Target model, when the operation has one.
    pub model: Option<Model>,
    /// Output canvas, when the operation has one.
    pub size: Option<ImageSize>,
    /// Sampling step count, when the operation has one.
    pub steps: Option<u32>,
    /// Sampler, when the operation has one.
    pub sampler: Option<Sampler>,
    /// Whether SMEA is enabled.
    pub smea: bool,
    /// Whether dynamic SMEA is enabled.
    pub smea_dyn: bool,
    /// Number of images requested.
    pub n_samples: u32,
    /// Whether the request carried an image-to-image source.
    pub has_img2img: bool,
    /// Whether the request carried an inpaint mask.
    pub has_inpaint_mask: bool,
    /// Image-to-image strength, when present.
    pub img2img_strength: Option<f32>,
    /// Number of vibe transfer references.
    pub vibe_reference_count: u32,
    /// Number of director-style character references.
    pub character_reference_count: u32,
    /// Number of enabled character prompts.
    pub character_count: u32,
    /// Character length of the processed positive prompt.
    pub prompt_len_chars: u32,
    /// Character length of the processed negative prompt.
    pub negative_prompt_len_chars: u32,
}

/// Classification of a failed request, derived from [`BridgeError`].
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestFailure {
    /// Local validation rejected the request; nothing was sent.
    InvalidRequest,
    /// The API returned a non-success status.
    Api {
        /// HTTP status code.
        status: u16,
    },
    /// A transport-level operation failed.
    Transport,
    /// A response payload could not be decoded.
    Decode,
    /// PNG metadata handling failed.
    Metadata,
}

impl RequestFailure {
    /// Classifies a [`BridgeError`] without retaining its payload.
    #[must_use]
    pub const fn from_error(error: &BridgeError) -> Self {
        match error {
            #[cfg(feature = "tokenizer")]
            BridgeError::Tokenizer(_) => Self::InvalidRequest,
            BridgeError::InvalidRequest(_) => Self::InvalidRequest,
            BridgeError::Api(api) => Self::Api { status: api.status },
            BridgeError::Transport(_) => Self::Transport,
            BridgeError::Decode(_) => Self::Decode,
            BridgeError::Metadata(_) => Self::Metadata,
        }
    }
}

/// How an observed request finished.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestOutcome {
    /// The request succeeded and returned `image_count` images.
    Succeeded {
        /// Number of images decoded from the response. `0` for non-image operations.
        image_count: u32,
    },
    /// The request failed.
    Failed {
        /// Failure classification.
        failure: RequestFailure,
    },
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

/// Account credit state at the moment it was sampled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditBalance {
    /// Combined fixed and purchased Anlas balance.
    pub anlas: i64,
    /// Whether the account is effectively on Opus tier.
    pub is_opus: bool,
    /// Numeric tier value.
    pub tier: i32,
    /// Whether the paid subscription was active when sampled.
    pub subscription_active: bool,
    /// Free V5 generation allowance, when reported.
    pub v5_usage: Option<V5UsageStatus>,
}

impl From<&SubscriptionInfo> for CreditBalance {
    fn from(info: &SubscriptionInfo) -> Self {
        Self {
            anlas: info.anlas_balance,
            is_opus: info.is_opus,
            tier: info.tier,
            subscription_active: info.subscription_active,
            v5_usage: info.v5_usage,
        }
    }
}

impl From<&CreditBalance> for AnlasPricingContext {
    fn from(balance: &CreditBalance) -> Self {
        Self {
            tier: balance.tier,
            subscription_active: balance.subscription_active,
            v5_usage_is_negative: balance.v5_usage.is_some_and(|usage| usage.is_negative),
            free_first_image_used: false,
        }
    }
}

impl From<&SubscriptionInfo> for AnlasPricingContext {
    fn from(info: &SubscriptionInfo) -> Self {
        Self {
            tier: info.tier,
            subscription_active: info.subscription_active,
            v5_usage_is_negative: info.v5_usage.is_some_and(|usage| usage.is_negative),
            free_first_image_used: false,
        }
    }
}

/// Cost shape attached to an observed request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestCostEstimate {
    /// Detailed image-generation estimate.
    Generation(AnlasEstimate),
    /// A request with one fixed Anlas price.
    Fixed {
        /// Estimated Anlas charge.
        anlas: u32,
    },
}

impl RequestCostEstimate {
    /// Returns the estimated total charge.
    #[must_use]
    pub const fn total(&self) -> u32 {
        match self {
            Self::Generation(estimate) => estimate.total,
            Self::Fixed { anlas } => *anlas,
        }
    }
}

/// Event reported to a [`CreditObserver`].
#[derive(Clone, Debug, PartialEq)]
pub enum CreditEvent {
    /// A request passed local validation and is about to be sent.
    RequestPlanned {
        /// De-identified request description.
        fingerprint: RequestFingerprint,
        /// Locally computed cost estimate.
        estimate: RequestCostEstimate,
    },
    /// A previously planned request finished.
    RequestSettled {
        /// De-identified request description, matching the planned event by `request_id`.
        fingerprint: RequestFingerprint,
        /// How the request finished.
        outcome: RequestOutcome,
    },
    /// The account balance was read from `/user/subscription`.
    BalanceSampled {
        /// Balance as reported by the API.
        balance: CreditBalance,
    },
}

/// Receives credit-related events from a [`crate::Client`].
///
/// Implementations must not block for long: the client awaits `observe` inline with the request.
#[async_trait]
pub trait CreditObserver: Send + Sync {
    /// Handles one event. Errors are the implementation's responsibility; the client ignores them.
    async fn observe(&self, event: CreditEvent);
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
/// [`GenerateImageRequest`]. Use [`GenerateImageRequest::anlas_estimate_input`] when a request is
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
        Self {
            model: Model::default(),
            size: ImageSize::default(),
            steps: 23,
            sampler: Sampler::default(),
            smea: false,
            smea_dyn: false,
            n_samples: 1,
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

impl AnlasEstimateInput {
    /// Builds pricing input from an assembled generation request.
    #[must_use]
    pub fn for_request(request: &GenerateImageRequest, context: AnlasPricingContext) -> Self {
        let capabilities = request.core.model.capabilities();
        let references = request.references.as_ref();
        let has_inpaint_mask = request
            .img2img
            .as_ref()
            .is_some_and(crate::Img2ImgRequest::is_infill)
            && capabilities.supports_inpainting;
        let character_references_supported = capabilities.supports_character_reference
            && (!has_inpaint_mask || capabilities.supports_character_reference_inpainting);
        let character_reference_count = if character_references_supported {
            references.map_or(0, |set| {
                u32::try_from(set.character_references.len()).unwrap_or(u32::MAX)
            })
        } else {
            0
        };
        let vibe_references_supported = capabilities.supports_encoded_vibe
            && !has_inpaint_mask
            && character_reference_count == 0;

        Self {
            model: request.core.model,
            size: request.core.size,
            steps: request.sampling.steps,
            sampler: request.sampling.sampler,
            smea: capabilities.supports_smea && request.guidance.smea,
            smea_dyn: capabilities.supports_smea && request.guidance.smea_dyn,
            n_samples: request.core.n_samples,
            img2img_strength: request
                .img2img
                .as_ref()
                .filter(|_| !has_inpaint_mask)
                .map(|img2img| img2img.strength),
            inpaint_img2img_strength: request
                .img2img
                .as_ref()
                .filter(|_| has_inpaint_mask)
                .map(|img2img| img2img.strength),
            has_inpaint_mask,
            character_reference_count,
            vibe_reference_count: if vibe_references_supported {
                references
                    .and_then(|set| set.vibe_transfer.as_ref())
                    .map_or(0, |vibe| {
                        u32::try_from(vibe.references.len()).unwrap_or(u32::MAX)
                    })
            } else {
                0
            },
            pending_vibe_encodes: 0,
            context,
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
    let capabilities = input.model.capabilities();
    let base_cost = base * smea_multiplier * capabilities.anlas_cost_multiplier;
    let strength = effective_strength(&input);
    let per_image = saturating_u32((base_cost * strength).ceil()).max(2);
    let status = if per_image > MAX_PER_IMAGE_COST {
        AnlasEstimateStatus::TooExpensive
    } else {
        AnlasEstimateStatus::Available
    };

    let requested_samples = input.n_samples;
    let api_samples = requested_samples.clamp(1, MAX_REQUEST_SAMPLES);
    let sample_limit = pricing_sample_limit(pixels).min(MAX_REQUEST_SAMPLES);
    let priced_samples = api_samples.min(sample_limit);
    let character_reference_count = chargeable_character_reference_count(&input);
    let character_reference = character_reference_count
        .saturating_mul(CHARACTER_REFERENCE_COST)
        .saturating_mul(priced_samples);
    let vibe_encoding = if capabilities.supports_encoded_vibe {
        input.pending_vibe_encodes.saturating_mul(VIBE_ENCODE_COST)
    } else {
        0
    };
    let vibe_reference_overage = if capabilities.supports_encoded_vibe {
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

impl GenerateImageRequest {
    /// Returns the unified pricing input represented by this request.
    #[must_use]
    pub fn anlas_estimate_input(&self, context: AnlasPricingContext) -> AnlasEstimateInput {
        AnlasEstimateInput::for_request(self, context)
    }

    /// Estimates this assembled request with the supplied account context.
    #[must_use]
    pub fn estimate_anlas_cost(&self, context: AnlasPricingContext) -> AnlasEstimate {
        estimate_anlas_cost(self.anlas_estimate_input(context))
    }
}

impl GenerateImageStreamRequest {
    /// Estimates the generation represented by this stream request.
    #[must_use]
    pub fn estimate_anlas_cost(&self, context: AnlasPricingContext) -> AnlasEstimate {
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
    let capabilities = input.model.capabilities();
    let references_allowed = capabilities.supports_character_reference
        && (!input.has_inpaint_mask || capabilities.supports_character_reference_inpainting);
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
    let capabilities = input.model.capabilities();
    input.context.tier >= 3
        && input.context.subscription_active
        && !input.context.free_first_image_used
        && character_reference_count == 0
        && pixels <= OPUS_FREE_PIXELS
        && input.steps <= OPUS_FREE_STEPS
        && !(capabilities.has_opus_usage_limit && input.context.v5_usage_is_negative)
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
