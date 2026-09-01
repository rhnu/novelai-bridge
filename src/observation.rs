//! Optional observation of request cost and account credit.
//!
//! A [`CreditObserver`] receives de-identified events for every request the client sends. The
//! crate never persists anything itself: it reports what was requested, what it estimates the
//! request costs, and what the account balance looked like when it was last sampled. Callers
//! combine those to build usage logs or credit models.

use async_trait::async_trait;

use crate::{
    AnlasEstimate, AnlasPricingContext, BridgeError, ImageSize, Model, Sampler, SubscriptionInfo,
    V5UsageStatus,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObservedOperation {
    /// `/ai/generate-image`
    GenerateImage,
    /// `/ai/generate-image-stream`
    #[cfg(feature = "streaming")]
    GenerateImageStream,
    /// `/ai/encode-vibe`
    EncodeVibe,
    /// `/ai/augment-image`
    AugmentImage,
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
    /// The caller explicitly cancelled consumption before a terminal stream event.
    #[cfg(feature = "streaming")]
    Cancelled,
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
    #[cfg_attr(not(feature = "credit-observer"), allow(dead_code))]
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
