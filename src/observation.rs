//! Optional request cost and account observation.

use async_trait::async_trait;

use crate::{
    AnlasEstimate, AnlasEstimateInput, BridgeError, ImageSize, Model, Sampler, SubscriptionInfo,
};

/// Cost-bearing generation properties without prompt text or image payloads.
#[derive(Clone, Debug, PartialEq)]
pub struct GenerationSummary {
    pub pricing: AnlasEstimateInput,
    pub sampler: Sampler,
    pub character_count: u32,
    pub prompt_len_chars: u32,
    pub negative_prompt_len_chars: u32,
}

/// Director operation without user-provided prompt text.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ObservedDirectorTool {
    Lineart,
    Sketch,
    BgRemoval,
    Emotion,
    Declutter,
    DeclutterKeepBubbles,
    Colorize,
}

/// Description of an operation containing only its applicable fields.
#[derive(Clone, Debug, PartialEq)]
pub enum ObservedOperation {
    GenerateImage(GenerationSummary),
    GenerateImageStream(GenerationSummary),
    EncodeVibe {
        model: Model,
    },
    AugmentImage {
        tool: ObservedDirectorTool,
        size: ImageSize,
    },
    UpscaleImage {
        model: Model,
        size: ImageSize,
        declared_blur_sigma: crate::UpscaleBlurSigma,
    },
}

/// De-identified operation paired with a client-local identifier.
#[derive(Clone, Debug, PartialEq)]
pub struct RequestFingerprint {
    pub request_id: u64,
    pub operation: ObservedOperation,
}

/// Classification of a failed request without its original payload.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestFailure {
    Stream,
    InvalidRequest,
    Preparation,
    Api { status: u16 },
    Transport,
    Encode,
    Decode,
    Metadata,
}

impl RequestFailure {
    #[must_use]
    pub const fn from_error(error: &BridgeError) -> Self {
        match error {
            BridgeError::Stream(_) => Self::Stream,
            #[cfg(feature = "tokenizer")]
            BridgeError::Tokenizer(_) => Self::InvalidRequest,
            BridgeError::InvalidRequest(_) => Self::InvalidRequest,
            BridgeError::Preparation(_) => Self::Preparation,
            BridgeError::Api(api) => Self::Api {
                status: api.status(),
            },
            BridgeError::Transport(_) => Self::Transport,
            BridgeError::Encode(_) => Self::Encode,
            BridgeError::Decode(_) => Self::Decode,
            BridgeError::Metadata(_) => Self::Metadata,
        }
    }
}

/// Terminal state of an observed operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestOutcome {
    /// Decoded images for ordinary generation, or distinct samples received from a stream.
    Succeeded {
        image_count: u32,
    },
    Failed {
        failure: RequestFailure,
    },
    Cancelled,
}

/// Cost shape attached to an observed request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RequestCostEstimate {
    Generation(AnlasEstimate),
    Fixed {
        anlas: u32,
    },
    /// The provider's published local pricing rule does not cover this input.
    Unavailable,
}

impl RequestCostEstimate {
    #[must_use]
    pub const fn total(&self) -> Option<u32> {
        match self {
            Self::Generation(estimate) => Some(estimate.total),
            Self::Fixed { anlas } => Some(*anlas),
            Self::Unavailable => None,
        }
    }
}

/// Request lifecycle and account samples reported to an observer.
#[derive(Clone, Debug, PartialEq)]
pub enum CreditEvent {
    RequestPlanned {
        fingerprint: RequestFingerprint,
        estimate: RequestCostEstimate,
    },
    RequestSettled {
        fingerprint: RequestFingerprint,
        outcome: RequestOutcome,
    },
    BalanceSampled {
        subscription: SubscriptionInfo,
    },
}

/// Receives events inline with request execution and must avoid long blocking work.
#[async_trait]
pub trait CreditObserver: Send + Sync {
    async fn observe(&self, event: CreditEvent);
}
