#[cfg(feature = "streaming")]
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::{
    BridgeError, CreditBalance, CreditEvent, NormalizedGeneration, ObservedOperation,
    RequestCostEstimate, RequestFailure, RequestFingerprint, RequestOutcome, SubscriptionInfo,
    Transport,
};

use super::Client;

/// A request that has been reported as planned and still needs a settled event.
pub(super) struct PendingObservation {
    fingerprint: RequestFingerprint,
}

/// Observer and fingerprint moved into a live stream until its real terminal state.
#[cfg(feature = "streaming")]
pub struct StreamObservation {
    observer: Arc<dyn crate::CreditObserver>,
    fingerprint: RequestFingerprint,
}

#[cfg(feature = "streaming")]
impl StreamObservation {
    pub async fn settle(self, outcome: RequestOutcome) {
        self.observer
            .observe(CreditEvent::RequestSettled {
                fingerprint: self.fingerprint,
                outcome,
            })
            .await;
    }
}

impl<T: Transport> Client<T> {
    #[cfg(feature = "streaming")]
    pub(super) fn stream_observation(
        &self,
        observation: Option<PendingObservation>,
    ) -> Option<StreamObservation> {
        Some(StreamObservation {
            observer: self.credit_observer.as_ref()?.clone(),
            fingerprint: observation?.fingerprint,
        })
    }
    /// Reports a planned generation request, returning the handle used to settle it.
    ///
    /// Returns `None` when no observer is attached, so fingerprint construction is skipped.
    pub(super) async fn begin_generation_observation(
        &self,
        normalized: &NormalizedGeneration,
        operation: ObservedOperation,
    ) -> Option<PendingObservation> {
        let observer = self.credit_observer.as_ref()?;

        let params = normalized.request();
        let context = self.last_pricing_context();
        let references = params.references.as_ref();
        let vibe_reference_count = references
            .and_then(|set| set.vibe_transfer.as_ref())
            .map_or(0, |vibe| vibe.references.len());
        let character_reference_count = references.map_or(0, |set| set.character_references.len());
        let character_count = params
            .characters
            .as_ref()
            .map_or(0, crate::CharacterSet::enabled_count);
        let resolved = normalized.resolved_prompts();

        let fingerprint = RequestFingerprint {
            request_id: self.next_request_id(),
            operation,
            model: Some(params.core.model),
            size: Some(params.core.size),
            steps: Some(params.sampling.steps),
            sampler: Some(params.sampling.sampler),
            smea: params.guidance.smea,
            smea_dyn: params.guidance.smea_dyn,
            n_samples: params.core.n_samples,
            has_img2img: params.img2img.is_some(),
            has_inpaint_mask: params
                .img2img
                .as_ref()
                .is_some_and(crate::Img2ImgRequest::is_infill),
            img2img_strength: params.img2img.as_ref().map(|img2img| img2img.strength),
            vibe_reference_count: truncate_count(vibe_reference_count),
            character_reference_count: truncate_count(character_reference_count),
            character_count: truncate_count(character_count),
            prompt_len_chars: truncate_count(resolved.prompt.chars().count()),
            negative_prompt_len_chars: truncate_count(resolved.negative_prompt.chars().count()),
        };

        let estimate = normalized.estimate_anlas_cost(context);

        observer
            .observe(CreditEvent::RequestPlanned {
                fingerprint: fingerprint.clone(),
                estimate: RequestCostEstimate::Generation(estimate),
            })
            .await;

        Some(PendingObservation { fingerprint })
    }

    /// Reports a planned non-generation request.
    pub(super) async fn begin_simple_observation(
        &self,
        operation: ObservedOperation,
        estimate: RequestCostEstimate,
    ) -> Option<PendingObservation> {
        let observer = self.credit_observer.as_ref()?;
        let fingerprint = RequestFingerprint {
            request_id: self.next_request_id(),
            operation,
            model: None,
            size: None,
            steps: None,
            sampler: None,
            smea: false,
            smea_dyn: false,
            n_samples: 1,
            has_img2img: false,
            has_inpaint_mask: false,
            img2img_strength: None,
            vibe_reference_count: 0,
            character_reference_count: 0,
            character_count: 0,
            prompt_len_chars: 0,
            negative_prompt_len_chars: 0,
        };

        observer
            .observe(CreditEvent::RequestPlanned {
                fingerprint: fingerprint.clone(),
                estimate,
            })
            .await;

        Some(PendingObservation { fingerprint })
    }

    pub(super) async fn finish_observation_succeeded(
        &self,
        observation: Option<PendingObservation>,
        image_count: u32,
    ) {
        self.settle_observation(observation, RequestOutcome::Succeeded { image_count })
            .await;
    }

    pub(super) async fn finish_observation_failed(
        &self,
        observation: Option<PendingObservation>,
        error: &BridgeError,
    ) {
        self.settle_observation(
            observation,
            RequestOutcome::Failed {
                failure: RequestFailure::from_error(error),
            },
        )
        .await;
    }

    async fn settle_observation(
        &self,
        observation: Option<PendingObservation>,
        outcome: RequestOutcome,
    ) {
        let (Some(observer), Some(observation)) = (self.credit_observer.as_ref(), observation)
        else {
            return;
        };
        observer
            .observe(CreditEvent::RequestSettled {
                fingerprint: observation.fingerprint,
                outcome,
            })
            .await;
    }

    /// Records and reports a freshly read account balance.
    pub(super) async fn observe_balance(&self, info: &SubscriptionInfo) {
        let balance = CreditBalance::from(info);
        if let Ok(mut last) = self.last_balance.lock() {
            *last = Some(balance.clone());
        }
        let Some(observer) = self.credit_observer.as_ref() else {
            return;
        };
        observer
            .observe(CreditEvent::BalanceSampled { balance })
            .await;
    }

    fn next_request_id(&self) -> u64 {
        self.request_counter.fetch_add(1, Ordering::Relaxed)
    }
}

fn truncate_count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
