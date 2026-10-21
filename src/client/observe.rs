#[cfg(feature = "credit-observer")]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use super::Client;
use crate::{BridgeError, NormalizedGeneration};
#[cfg(feature = "credit-observer")]
use crate::{
    CreditEvent, CreditObserver, GenerationSummary, ObservedDirectorTool, ObservedOperation,
    RequestCostEstimate, RequestFailure, RequestFingerprint, RequestOutcome, SubscriptionInfo,
};

/// Holds the observer only while an observed operation remains pending.
#[derive(Default)]
pub(super) struct PendingObservation {
    #[cfg(feature = "credit-observer")]
    pending: Option<(Arc<dyn CreditObserver>, RequestFingerprint)>,
}

impl PendingObservation {
    pub(super) async fn succeed(self, image_count: u32) {
        #[cfg(feature = "credit-observer")]
        self.settle(RequestOutcome::Succeeded { image_count }).await;
        #[cfg(not(feature = "credit-observer"))]
        let _ = image_count;
    }

    pub(super) async fn fail(self, error: &BridgeError) {
        #[cfg(feature = "credit-observer")]
        self.settle(RequestOutcome::Failed {
            failure: RequestFailure::from_error(error),
        })
        .await;
        #[cfg(not(feature = "credit-observer"))]
        let _ = error;
    }

    #[cfg(all(feature = "streaming", feature = "credit-observer"))]
    pub(super) async fn cancel(self) { self.settle(RequestOutcome::Cancelled).await; }

    #[cfg(feature = "credit-observer")]
    async fn settle(self, outcome: RequestOutcome) {
        if let Some((observer, fingerprint)) = self.pending {
            observer
                .observe(CreditEvent::RequestSettled {
                    fingerprint,
                    outcome,
                })
                .await;
        }
    }
}

#[cfg(feature = "credit-observer")]
#[derive(Clone)]
pub(super) struct ObservationContext {
    observer: Arc<dyn CreditObserver>,
    counter: Arc<AtomicU64>,
}

#[cfg(feature = "credit-observer")]
impl ObservationContext {
    pub(super) fn new(observer: Arc<dyn CreditObserver>) -> Self {
        Self {
            observer,
            counter: Arc::new(AtomicU64::new(1)),
        }
    }

    pub(super) async fn begin(
        &self,
        operation: ObservedOperation,
        estimate: RequestCostEstimate,
    ) -> PendingObservation {
        let fingerprint = RequestFingerprint {
            request_id: self.counter.fetch_add(1, Ordering::Relaxed),
            operation,
        };
        self.observer
            .observe(CreditEvent::RequestPlanned {
                fingerprint: fingerprint.clone(),
                estimate,
            })
            .await;
        PendingObservation {
            pending: Some((Arc::clone(&self.observer), fingerprint)),
        }
    }

    pub(super) async fn observe_subscription(&self, info: &SubscriptionInfo) {
        self.observer
            .observe(CreditEvent::BalanceSampled {
                subscription: info.clone(),
            })
            .await;
    }
}

impl<T: crate::Transport + ?Sized> Client<T> {
    pub(super) async fn observe_generation(
        &self,
        normalized: &NormalizedGeneration,
        streaming: bool,
    ) -> PendingObservation {
        #[cfg(feature = "credit-observer")]
        if let Some(context) = &self.observation {
            let pricing = normalized.anlas_estimate_input(self.last_pricing_context());
            let summary = generation_summary(normalized, pricing);
            let operation = if streaming {
                #[cfg(feature = "streaming")]
                {
                    ObservedOperation::GenerateImageStream(summary)
                }
                #[cfg(not(feature = "streaming"))]
                {
                    ObservedOperation::GenerateImage(summary)
                }
            } else {
                ObservedOperation::GenerateImage(summary)
            };
            return context
                .begin(
                    operation,
                    RequestCostEstimate::Generation(crate::estimate_anlas_cost(pricing)),
                )
                .await;
        }
        let _ = (normalized, streaming);
        PendingObservation::default()
    }

    pub(super) async fn observe_director(
        &self,
        prepared: &crate::PreparedDirector,
    ) -> PendingObservation {
        #[cfg(feature = "credit-observer")]
        if let Some(context) = &self.observation {
            let tool = match prepared.command() {
                crate::DirectorCommand::Lineart => ObservedDirectorTool::Lineart,
                crate::DirectorCommand::Sketch => ObservedDirectorTool::Sketch,
                crate::DirectorCommand::BgRemoval => ObservedDirectorTool::BgRemoval,
                crate::DirectorCommand::Emotion { .. } => ObservedDirectorTool::Emotion,
                crate::DirectorCommand::Declutter => ObservedDirectorTool::Declutter,
                crate::DirectorCommand::Colorize { .. } => ObservedDirectorTool::Colorize,
            };
            let size = prepared.image_size();
            return context
                .begin(
                    ObservedOperation::AugmentImage { tool, size },
                    RequestCostEstimate::Fixed {
                        anlas: crate::estimate_director_tool_cost(
                            size,
                            self.last_pricing_context(),
                        ),
                    },
                )
                .await;
        }
        let _ = prepared;
        PendingObservation::default()
    }

    pub(super) async fn observe_vibe(&self, prepared: &crate::PreparedVibe) -> PendingObservation {
        #[cfg(feature = "credit-observer")]
        if let Some(context) = &self.observation {
            return context
                .begin(
                    ObservedOperation::EncodeVibe {
                        model: prepared.model(),
                    },
                    RequestCostEstimate::Fixed {
                        anlas: crate::pricing::VIBE_ENCODE_COST,
                    },
                )
                .await;
        }
        let _ = prepared;
        PendingObservation::default()
    }
}

#[cfg(feature = "credit-observer")]
fn generation_summary(
    normalized: &NormalizedGeneration,
    pricing: crate::AnlasEstimateInput,
) -> GenerationSummary {
    let params = normalized.request();
    let prompts = normalized.resolved_prompts();
    GenerationSummary {
        pricing,
        sampler: params.sampling.sampler,
        character_count: count(
            params
                .characters
                .as_ref()
                .map_or(0, crate::CharacterSet::enabled_count),
        ),
        prompt_len_chars: count(prompts.prompt.chars().count()),
        negative_prompt_len_chars: count(prompts.negative_prompt.chars().count()),
    }
}

#[cfg(feature = "credit-observer")]
fn count(value: usize) -> u32 { u32::try_from(value).unwrap_or(u32::MAX) }
