use super::{Field, Normalizer};
use crate::{BridgeError, GenerateImageRequest, GuidanceField, ModelDescriptor, NoiseSchedule};

impl Normalizer {
    pub(super) fn normalize_sampling(
        &mut self,
        request: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        let profile = descriptor.sampling();
        let sampler = profile.resolve_sampler_for_steps(
            request.mode(),
            request.sampling.sampler,
            request.sampling.steps,
        );
        if sampler != request.sampling.sampler {
            self.require_capability(
                false,
                Field::Sampler,
                "a model and operation supporting this sampler",
                super::NormalizationChange::Fallback,
            )?;
            request.sampling.sampler = sampler;
        }
        let schedule = profile
            .resolve_noise_schedule_for(sampler, request.sampling.noise_schedule)
            .unwrap_or(NoiseSchedule::Native);
        if schedule != request.sampling.noise_schedule {
            self.require_capability(
                false,
                Field::NoiseSchedule,
                "a model and sampler supporting this noise schedule",
                super::NormalizationChange::Fallback,
            )?;
            request.sampling.noise_schedule = schedule;
        }
        if (request.guidance.smea || request.guidance.smea_dyn)
            && !profile.supports_smea(request.mode(), sampler)
        {
            self.require_capability(
                false,
                Field::Guidance(GuidanceField::Smea),
                "a model, operation and sampler supporting SMEA",
                super::NormalizationChange::Unsupported,
            )?;
            request.guidance.smea = false;
            request.guidance.smea_dyn = false;
        }
        if request.guidance.smea_dyn && !request.guidance.smea {
            self.require_capability(
                false,
                Field::Guidance(GuidanceField::SmeaDyn),
                "SMEA enabled alongside dynamic SMEA",
                super::NormalizationChange::Unsupported,
            )?;
            request.guidance.smea_dyn = false;
        }
        Ok(())
    }
}
