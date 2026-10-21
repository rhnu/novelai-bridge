use std::sync::Arc;

use super::Client;
use crate::{
    BridgeError, CreditEvent, CreditObserver, GenerationSummary, NormalizedGeneration,
    ObservedDirectorTool, ObservedOperation, RequestCostEstimate, RequestFailure,
    RequestFingerprint, RequestOutcome, SubscriptionInfo,
};

/// Holds the observer only while an observed operation remains pending.
#[derive(Default)]
pub(super) struct PendingObservation {
    pending: Option<(Arc<dyn CreditObserver>, RequestFingerprint)>,
}

impl PendingObservation {
    pub(super) async fn succeed(self, image_count: u32) {
        self.settle(RequestOutcome::Succeeded { image_count }).await;
    }

    pub(super) fn fail(self, error: &BridgeError) -> impl Future<Output = ()> + Send + 'static {
        let failure = RequestFailure::from_error(error);
        async move {
            self.settle(RequestOutcome::Failed { failure }).await;
        }
    }

    pub(super) async fn cancel(self) { self.settle(RequestOutcome::Cancelled).await; }

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

#[derive(Clone)]
pub(super) struct ObservationContext {
    observer: Arc<dyn CreditObserver>,
}

impl ObservationContext {
    pub(super) fn new(observer: Arc<dyn CreditObserver>) -> Self { Self { observer } }

    pub(super) async fn begin(
        &self,
        request_id: u64,
        operation: ObservedOperation,
        estimate: RequestCostEstimate,
    ) -> PendingObservation {
        let fingerprint = RequestFingerprint {
            request_id,
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
        delivery: crate::GenerationDelivery,
        request_id: u64,
    ) -> PendingObservation {
        if let Some(context) = &self.observation {
            let pricing = normalized.anlas_estimate_input(self.last_pricing_context());
            let summary = generation_summary(normalized, pricing);
            let operation = match delivery {
                crate::GenerationDelivery::Images => ObservedOperation::GenerateImage(summary),
                crate::GenerationDelivery::Stream(_) => {
                    ObservedOperation::GenerateImageStream(summary)
                },
            };
            return context
                .begin(
                    request_id,
                    operation,
                    RequestCostEstimate::Generation(crate::estimate_anlas_cost(pricing)),
                )
                .await;
        }
        PendingObservation::default()
    }

    pub(super) async fn observe_director(
        &self,
        prepared: &crate::PreparedDirector,
        request_id: u64,
    ) -> PendingObservation {
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
                    request_id,
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
        PendingObservation::default()
    }

    pub(super) async fn observe_vibe(
        &self,
        prepared: &crate::PreparedVibe,
        request_id: u64,
    ) -> PendingObservation {
        if let Some(context) = &self.observation {
            return context
                .begin(
                    request_id,
                    ObservedOperation::EncodeVibe {
                        model: prepared.model(),
                    },
                    RequestCostEstimate::Fixed {
                        anlas: crate::pricing::VIBE_ENCODE_COST,
                    },
                )
                .await;
        }
        PendingObservation::default()
    }
}

fn generation_summary(
    normalized: &NormalizedGeneration,
    pricing: crate::AnlasEstimateInput,
) -> GenerationSummary {
    let params = normalized.request();
    let prompts = normalized.resolved_prompts();
    GenerationSummary {
        pricing,
        sampler: params.sampling.sampler,
        character_count: count(prompts.characters.len()),
        prompt_len_chars: count(prompts.prompt.chars().count()),
        negative_prompt_len_chars: count(prompts.negative_prompt.chars().count()),
    }
}

fn count(value: usize) -> u32 { u32::try_from(value).unwrap_or(u32::MAX) }
