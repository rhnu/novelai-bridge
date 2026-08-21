use std::sync::atomic::Ordering;

use crate::{
    AnlasEstimateInput, BridgeError, CreditBalance, CreditEvent, GenerateImageRequest,
    ObservedOperation, RequestFailure, RequestFingerprint, RequestOutcome, SubscriptionInfo,
    Transport, estimate_anlas_cost,
};

use super::Client;

/// A request that has been reported as planned and still needs a settled event.
pub(super) struct PendingObservation {
    fingerprint: RequestFingerprint,
}

impl<T: Transport> Client<T> {
    /// Reports a planned generation request, returning the handle used to settle it.
    ///
    /// Returns `None` when no observer is attached, so fingerprint construction is skipped.
    pub(super) async fn begin_generation_observation(
        &self,
        params: &GenerateImageRequest,
        operation: ObservedOperation,
    ) -> Option<PendingObservation> {
        let observer = self.credit_observer.as_ref()?;

        let references = params.references.as_ref();
        let vibe_reference_count = references
            .and_then(|set| set.vibe_transfer.as_ref())
            .map_or(0, |vibe| vibe.references.len());
        let character_reference_count = references.map_or(0, |set| set.character_references.len());
        let character_count = params
            .characters
            .as_ref()
            .map_or(0, crate::CharacterSet::enabled_count);

        let fingerprint = RequestFingerprint {
            request_id: self.next_request_id(),
            operation,
            model: Some(params.core.model),
            size: Some(params.core.size),
            steps: Some(params.sampling.steps),
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
            prompt_len_chars: truncate_count(params.processed_prompt().chars().count()),
            negative_prompt_len_chars: truncate_count(
                params.processed_negative_prompt().chars().count(),
            ),
        };

        let estimate = estimate_anlas_cost(AnlasEstimateInput {
            size: params.core.size,
            steps: params.sampling.steps,
            n_samples: params.core.n_samples,
            img2img_strength: fingerprint.img2img_strength,
            character_reference_count: fingerprint.character_reference_count,
            pending_vibe_encodes: 0,
            is_opus: self.last_known_opus(),
        });

        observer
            .observe(CreditEvent::RequestPlanned {
                fingerprint: fingerprint.clone(),
                estimate,
            })
            .await;

        Some(PendingObservation { fingerprint })
    }

    /// Reports a planned non-generation request.
    pub(super) async fn begin_simple_observation(
        &self,
        operation: ObservedOperation,
    ) -> Option<PendingObservation> {
        let observer = self.credit_observer.as_ref()?;
        let fingerprint = RequestFingerprint {
            request_id: self.next_request_id(),
            operation,
            model: None,
            size: None,
            steps: None,
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
                estimate: crate::AnlasEstimate::default(),
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

    fn last_known_opus(&self) -> bool {
        self.last_balance
            .lock()
            .ok()
            .and_then(|last| last.as_ref().map(|balance| balance.is_opus))
            .unwrap_or(false)
    }
}

fn truncate_count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
