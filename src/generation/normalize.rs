mod characters;
mod prompting;
mod references;
mod sampling;
mod values;

use super::{NormalizationChange, NormalizationDiagnostic, NormalizedGeneration};
use crate::{
    BridgeError, GenerateImageRequest, GenerationConstraints, GenerationField as Field,
    GuidanceField, GuidanceParams, ImageInputField, InvalidRequest, ModelDescriptor,
    ValidationPolicy,
};

pub(super) fn normalize(
    mut request: GenerateImageRequest,
    policy: ValidationPolicy,
) -> Result<NormalizedGeneration, BridgeError> {
    request.policy = policy;
    let effective_model = request.descriptor().effective_model_for(request.mode());
    let mut diagnostics = Vec::new();
    if effective_model != request.core.model {
        if policy.is_reject() {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: Field::Model.into(),
                required_model: effective_model.as_str().to_owned(),
            }
            .into());
        }
        request.core.model = effective_model;
        diagnostics.push(NormalizationDiagnostic {
            field: Field::Model,
            change: NormalizationChange::Fallback,
        });
    }
    let descriptor = request.descriptor();
    let mut normalizer = Normalizer {
        constraints: *descriptor.constraints(),
        policy,
        diagnostics,
    };
    normalizer.normalize_base_fields(&mut request)?;
    normalizer.apply_capability_gates(&mut request, descriptor)?;
    let model_id = descriptor.model_id_for(request.mode()).ok_or_else(|| {
        InvalidRequest::UnsupportedModelFeature {
            feature: Field::ImageInput(ImageInputField::Mask).into(),
            required_model: "models with an inpainting variant".to_owned(),
        }
    })?;
    if let Some(characters) = request.characters.as_mut()
        && let Some(profile) = descriptor.prompt().characters
    {
        normalizer.normalize_character_positions(characters, profile.positioning)?;
    }
    if let Some(references) = request.references.as_mut() {
        normalizer.normalize_character_references(references)?;
    }
    normalizer.normalize_operation(&mut request.operation, descriptor)?;
    normalizer.normalize_sampling(&mut request, descriptor)?;
    let geometry = super::geometry::resolve(&request)?;
    let sample_limit = descriptor.sample_limit(geometry.pricing_size);
    if sample_limit == 0 {
        return Err(InvalidRequest::UnsupportedModelFeature {
            feature: Field::Size.into(),
            required_model: "a model supporting this pricing canvas".to_owned(),
        }
        .into());
    }
    request.core.n_samples = normalizer.normalize_u32_range(
        Field::Samples,
        request.core.n_samples,
        descriptor.constraints().samples.min,
        sample_limit,
    )?;
    let prompts = request.assemble_prompts();
    Ok(NormalizedGeneration {
        model_id,
        geometry,
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
        params.sampling.cfg_rescale = self.normalize_f32_range(
            Field::CfgRescale,
            params.sampling.cfg_rescale,
            limits.cfg_rescale.min,
            limits.cfg_rescale.max,
        )?;
        self.normalize_generation_canvas(params)?;
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
        self.normalize_custom_negative_prompts(params, descriptor)?;
        if params.core.legacy_uc
            && !self.require_capability(
                descriptor.prompt().legacy_uc,
                Field::LegacyUc,
                "a V4 model supporting legacy UC",
                NormalizationChange::Unsupported,
            )?
        {
            params.core.legacy_uc = false;
        }
        if !self.require_capability(
            descriptor
                .prompt()
                .quality_preset(params.core.quality)
                .is_some(),
            Field::Quality,
            "a model supporting this prompt option",
            NormalizationChange::Fallback,
        )? {
            params.core.quality = descriptor.defaults().quality;
        }
        if !self.require_capability(
            descriptor
                .prompt()
                .uc_preset(params.core.uc_preset)
                .is_some(),
            Field::UcPreset,
            "a model supporting this UC preset",
            NormalizationChange::Fallback,
        )? {
            params.core.uc_preset = descriptor.defaults().uc_preset;
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

#[cfg(test)]
mod tests;
