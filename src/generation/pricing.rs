use super::{GenerateImageRequest, GenerateImageStreamRequest, NormalizedGeneration};
use crate::{
    AnlasEstimate, AnlasEstimateInput, AnlasPricingContext, BridgeError, GenerationInput,
    PricingMode, estimate_anlas_cost,
};

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

impl NormalizedGeneration {
    /// Projects effective pricing fields without cloning image payloads or assembling prompts.
    #[must_use]
    pub fn anlas_estimate_input(&self, context: AnlasPricingContext) -> AnlasEstimateInput {
        let request = self.request();
        let references = request.references.as_ref();
        let mode = match &request.operation {
            GenerationInput::TextToImage => PricingMode::TextToImage,
            GenerationInput::Inpainting(input) => PricingMode::Inpainting {
                strength: input
                    .img2img
                    .as_ref()
                    .map_or(1.0, |parameters| parameters.strength),
            },
            GenerationInput::ImageToImage(input) => PricingMode::ImageToImage {
                strength: input.strength,
            },
        };
        AnlasEstimateInput {
            model: request.core.model,
            size: request.core.size,
            steps: request.sampling.steps,
            smea: request.guidance.smea,
            smea_dyn: request.guidance.smea_dyn,
            n_samples: request.core.n_samples,
            mode,
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

#[cfg(test)]
mod tests;
