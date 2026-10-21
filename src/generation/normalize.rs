mod characters;
mod references;
mod values;

use super::{NormalizationChange, NormalizationDiagnostic, NormalizedGeneration};
use crate::{
    BridgeError, GenerateImageRequest, GenerationConstraints, GenerationField as Field,
    GuidanceField, GuidanceParams, ImageInputField, InvalidRequest, ModelDescriptor, QualityPreset,
    ValidationPolicy,
};

pub(super) fn normalize(
    mut request: GenerateImageRequest,
    policy: ValidationPolicy,
) -> Result<NormalizedGeneration, BridgeError> {
    request.policy = policy;
    let descriptor = request.descriptor();
    let mut normalizer = Normalizer {
        constraints: *descriptor.constraints(),
        policy,
        diagnostics: Vec::new(),
    };
    validate_prompt(&request)?;
    normalizer.normalize_base_fields(&mut request)?;
    normalizer.apply_capability_gates(&mut request, descriptor)?;
    let model_id = descriptor.model_id_for(request.mode()).ok_or_else(|| {
        InvalidRequest::UnsupportedModelFeature {
            feature: Field::ImageInput(ImageInputField::Mask).into(),
            required_model: "models with an inpainting variant".to_owned(),
        }
    })?;
    if let Some(characters) = request.characters.as_mut() {
        Normalizer::validate_character_inputs(&characters.characters)?;
        if let Some(profile) = descriptor.prompt().characters {
            normalizer.normalize_character_positions(characters, profile.positioning)?;
        }
    }
    if let Some(references) = request.references.as_mut() {
        normalizer.normalize_character_references(references)?;
        normalizer.normalize_vibe_transfer(references.vibe_transfer.as_mut())?;
    }
    normalizer.normalize_operation(&mut request.operation)?;
    let schedule = descriptor
        .sampling()
        .resolve_noise_schedule(request.sampling.noise_schedule);
    if schedule != request.sampling.noise_schedule {
        normalizer.require_capability(
            false,
            Field::NoiseSchedule,
            "a model supporting the selected noise schedule",
            NormalizationChange::Fallback,
        )?;
        request.sampling.noise_schedule = schedule;
    }
    let prompts = request.assemble_prompts();
    Ok(NormalizedGeneration {
        model_id,
        request,
        prompts,
        diagnostics: normalizer.diagnostics,
    })
}

struct Normalizer {
    constraints: GenerationConstraints,
    policy: ValidationPolicy,
    diagnostics: Vec<NormalizationDiagnostic>,
}

impl Normalizer {
    fn record(&mut self, field: Field, change: NormalizationChange) {
        self.diagnostics
            .push(NormalizationDiagnostic { field, change });
    }

    #[expect(
        clippy::float_cmp,
        reason = "Diagnostics report exact value changes rather than approximate equivalence"
    )]
    fn adjusted(&mut self, field: Field, from: f64, to: f64) {
        if from != to {
            self.record(field, NormalizationChange::Adjusted { from, to });
        }
    }

    fn normalize_base_fields(
        &mut self,
        params: &mut GenerateImageRequest,
    ) -> Result<(), BridgeError> {
        let limits = self.constraints;
        params.sampling.steps = self.normalize_u32_range(
            Field::Steps,
            params.sampling.steps,
            limits.steps.min,
            limits.steps.max,
        )?;
        params.sampling.scale = self.normalize_f32_range(
            Field::Scale,
            params.sampling.scale,
            limits.scale.min,
            limits.scale.max,
        )?;
        params.core.n_samples = self.normalize_u32_range(
            Field::Samples,
            params.core.n_samples,
            limits.samples.min,
            limits.samples.max,
        )?;
        params.sampling.cfg_rescale = self.normalize_f32_range(
            Field::CfgRescale,
            params.sampling.cfg_rescale,
            limits.cfg_rescale.min,
            limits.cfg_rescale.max,
        )?;
        params.core.size.width =
            self.normalize_image_dimension(Field::Width, params.core.size.width)?;
        params.core.size.height =
            self.normalize_image_dimension(Field::Height, params.core.size.height)?;
        Ok(())
    }

    /// Drops or rejects request groups the selected model does not support.
    fn apply_capability_gates(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        self.gate_characters(params, descriptor)?;
        self.normalize_references(params, descriptor)?;
        if params.core.quality == QualityPreset::Light
            && !self.require_capability(
                descriptor.prompt().quality.supports(params.core.quality),
                Field::Quality,
                "a model supporting this prompt option",
                NormalizationChange::Fallback,
            )?
        {
            params.core.quality = QualityPreset::Standard;
        }
        if params.core.transparent_background
            && !self.require_capability(
                descriptor.prompt().transparent_background,
                Field::TransparentBackground,
                "a model supporting this prompt option",
                NormalizationChange::Unsupported,
            )?
        {
            params.core.transparent_background = false;
        }
        if params.core.furry_mode
            && !self.require_capability(
                descriptor.prompt().furry_mode,
                Field::FurryMode,
                "a model supporting structured character prompts",
                NormalizationChange::Unsupported,
            )?
        {
            params.core.furry_mode = false;
        }
        self.gate_guidance(&mut params.guidance, descriptor)?;
        Ok(())
    }

    // The descriptor owns support and precedence; this boundary only maps decisions to errors
    // or diagnostics, preserving the caller's Reject/Normalize policy.

    fn gate_guidance(
        &mut self,
        guidance: &mut GuidanceParams,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if guidance.variety_boost
            && !self.require_capability(
                descriptor.sampling().variety.is_some(),
                Field::Guidance(GuidanceField::VarietyBoost),
                "a model supporting variety boost",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.variety_boost = false;
        }

        if guidance.decrisp
            && !self.require_capability(
                descriptor.sampling().dynamic_thresholding,
                Field::Guidance(GuidanceField::Decrisp),
                "a model supporting this sampling option",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.decrisp = false;
        }

        if (guidance.smea || guidance.smea_dyn)
            && !self.require_capability(
                descriptor.sampling().smea,
                Field::Guidance(GuidanceField::Smea),
                "a model supporting this sampling option",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.smea = false;
            guidance.smea_dyn = false;
        }

        Ok(())
    }

    /// Returns whether the feature should be kept, honoring the validation policy.
    fn require_capability(
        &mut self,
        supported: bool,
        feature: Field,
        required_model: &str,
        change: NormalizationChange,
    ) -> Result<bool, BridgeError> {
        if supported {
            return Ok(true);
        }
        if self.policy.is_reject() {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: feature.into(),
                required_model: required_model.to_owned(),
            }
            .into());
        }
        self.record(feature, change);
        Ok(false)
    }
}

fn validate_prompt(params: &GenerateImageRequest) -> Result<(), BridgeError> {
    if params.core.prompt.trim().is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: Field::Prompt.into(),
        }
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
