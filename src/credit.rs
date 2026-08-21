//! Optional observation of request cost and account credit.
//!
//! A [`CreditObserver`] receives de-identified events for every request the client sends. The
//! crate never persists anything itself: it reports what was requested, what it estimates the
//! request costs, and what the account balance looked like when it was last sampled. Callers
//! combine those to build usage logs or credit models.

use async_trait::async_trait;

use crate::{BridgeError, ImageSize, Model, SubscriptionInfo, V5UsageStatus};

/// Anlas cost per `/ai/encode-vibe` call.
const VIBE_ENCODE_COST: u32 = 2;
/// Anlas cost per character reference, per generated image.
const CHARACTER_REFERENCE_COST: u32 = 5;
/// Opus free-generation pixel ceiling.
const OPUS_FREE_PIXELS: u64 = 1024 * 1024;
/// Opus free-generation step ceiling.
const OPUS_FREE_STEPS: u32 = 28;
/// Smallest canvas area used by the cost formula.
const MIN_COST_PIXELS: u64 = 65_536;
/// Linear area term of the `NovelAI` cost function.
const COST_AREA_TERM: f64 = 2.951_823_174_884_865e-6;
/// Area-times-steps term of the `NovelAI` cost function.
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

/// Account credit state at the moment it was sampled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreditBalance {
    /// Combined fixed and purchased Anlas balance.
    pub anlas: i64,
    /// Whether the account is effectively on Opus tier.
    pub is_opus: bool,
    /// Numeric tier value.
    pub tier: i32,
    /// Free V5 generation allowance, when reported.
    pub v5_usage: Option<V5UsageStatus>,
}

impl From<&SubscriptionInfo> for CreditBalance {
    fn from(info: &SubscriptionInfo) -> Self {
        Self {
            anlas: info.anlas_balance,
            is_opus: info.is_opus,
            tier: info.tier,
            v5_usage: info.v5_usage,
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
        estimate: AnlasEstimate,
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

/// Inputs to [`estimate_anlas_cost`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AnlasEstimateInput {
    /// Output canvas.
    pub size: ImageSize,
    /// Sampling step count.
    pub steps: u32,
    /// Number of images requested.
    pub n_samples: u32,
    /// Image-to-image strength, when the request has an image source.
    pub img2img_strength: Option<f32>,
    /// Number of director-style character references.
    pub character_reference_count: u32,
    /// Number of vibe references that still need encoding for this request.
    pub pending_vibe_encodes: u32,
    /// Whether the account is on Opus tier.
    pub is_opus: bool,
}

impl Default for AnlasEstimateInput {
    fn default() -> Self {
        Self {
            size: ImageSize::default(),
            steps: 23,
            n_samples: 1,
            img2img_strength: None,
            character_reference_count: 0,
            pending_vibe_encodes: 0,
            is_opus: false,
        }
    }
}

/// Breakdown of an estimated Anlas cost.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct AnlasEstimate {
    /// Cost of a single image before the Opus free allowance is applied.
    pub per_image: u32,
    /// Generation cost after the Opus free allowance is applied.
    pub generation: u32,
    /// Character reference surcharge.
    pub character_reference: u32,
    /// One-off vibe encoding cost.
    pub vibe_encoding: u32,
    /// Sum of all components.
    pub total: u32,
    /// Whether the estimate is fully covered by the free allowance.
    pub free: bool,
}

/// Estimates the Anlas cost of a generation request.
///
/// The formula mirrors the cost function used by the `NovelAI` web client. It is an estimate:
/// pair it with [`CreditEvent::BalanceSampled`] deltas to reconcile against actual charges.
#[must_use]
pub fn estimate_anlas_cost(input: AnlasEstimateInput) -> AnlasEstimate {
    let pixels = input.size.pixels().max(MIN_COST_PIXELS);
    #[allow(clippy::cast_precision_loss)]
    let pixels_f = pixels as f64;
    let base = (COST_AREA_STEP_TERM * pixels_f)
        .mul_add(f64::from(input.steps), COST_AREA_TERM * pixels_f)
        .ceil();
    let strength = f64::from(input.img2img_strength.unwrap_or(1.0));
    let per_image = saturating_u32((base * strength).ceil()).max(2);

    let free_eligible =
        input.is_opus && pixels <= OPUS_FREE_PIXELS && input.steps <= OPUS_FREE_STEPS;
    let generation = if free_eligible {
        0
    } else {
        per_image.saturating_mul(input.n_samples)
    };
    let character_reference = input
        .character_reference_count
        .saturating_mul(CHARACTER_REFERENCE_COST)
        .saturating_mul(input.n_samples);
    let vibe_encoding = input.pending_vibe_encodes.saturating_mul(VIBE_ENCODE_COST);
    let total = generation
        .saturating_add(character_reference)
        .saturating_add(vibe_encoding);

    AnlasEstimate {
        per_image,
        generation,
        character_reference,
        vibe_encoding,
        total,
        free: total == 0,
    }
}

/// Estimates the Anlas cost of a director tool or upscale call.
///
/// `NovelAI` charges these from a pixel-bucket table; Opus covers inputs up to 409600 pixels.
#[must_use]
pub const fn estimate_director_tool_cost(size: ImageSize, is_opus: bool) -> u32 {
    let pixels = size.pixels();
    if is_opus && pixels <= 409_600 {
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
