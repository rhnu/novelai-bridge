use super::{
    NormalizationChange, NormalizationDiagnostic, NormalizationPolicy, NormalizedGeneration,
};
use crate::{
    BridgeError, Character, CharacterSet, GenerateImageRequest, GenerationConstraints,
    GuidanceParams, ImageSize, Img2ImgRequest, InvalidRequest, ModelDescriptor, QualityPreset,
    ReferenceAvailability, ReferenceIntent, ReferenceSet, VibeTransferConfig,
    internal::request_helpers::{is_center_position, snap_coord_to_grid},
};

pub(super) fn normalize(
    mut request: GenerateImageRequest,
    policy: NormalizationPolicy,
) -> Result<NormalizedGeneration, BridgeError> {
    request.strict_mode = policy == NormalizationPolicy::Reject;
    let descriptor = request.descriptor();
    let mut normalizer = Normalizer {
        constraints: *descriptor.constraints(),
        reject: request.strict_mode,
        diagnostics: Vec::new(),
    };
    validate_prompt(&request)?;
    normalizer.normalize_base_fields(&mut request)?;
    normalizer.apply_capability_gates(&mut request, descriptor)?;
    if let Some(characters) = request.characters.as_mut() {
        normalizer.validate_character_inputs(&characters.characters)?;
        normalizer.normalize_character_positions(characters)?;
    }
    if let Some(references) = request.references.as_mut() {
        normalizer.normalize_character_references(references)?;
        normalizer.normalize_vibe_transfer(references.vibe_transfer.as_mut())?;
    }
    normalizer.normalize_img2img(request.img2img.as_mut())?;
    let schedule = descriptor
        .sampling()
        .resolve_noise_schedule(request.sampling.noise_schedule);
    if schedule != request.sampling.noise_schedule {
        normalizer.record(
            "sampling.noise_schedule".to_owned(),
            NormalizationChange::Fallback,
        );
        request.sampling.noise_schedule = schedule;
    }
    let prompts = request.assemble_prompts();
    Ok(NormalizedGeneration {
        request,
        prompts,
        diagnostics: normalizer.diagnostics,
    })
}

struct Normalizer {
    constraints: GenerationConstraints,
    reject: bool,
    diagnostics: Vec<NormalizationDiagnostic>,
}

impl Normalizer {
    fn record(&mut self, field: String, change: NormalizationChange) {
        self.diagnostics
            .push(NormalizationDiagnostic { field, change });
    }

    #[allow(clippy::float_cmp)] // Exact changes, not approximate numerical equivalence.
    fn adjusted(&mut self, field: &str, from: f64, to: f64) {
        if from != to {
            self.record(field.to_owned(), NormalizationChange::Adjusted { from, to });
        }
    }

    fn normalize_base_fields(
        &mut self,
        params: &mut GenerateImageRequest,
    ) -> Result<(), BridgeError> {
        let limits = self.constraints;
        params.sampling.steps = self.normalize_u32_range(
            "steps",
            params.sampling.steps,
            limits.steps.min,
            limits.steps.max,
        )?;
        params.sampling.scale = self.normalize_f32_range(
            "scale",
            params.sampling.scale,
            limits.scale.min,
            limits.scale.max,
        )?;
        params.core.n_samples = self.normalize_u32_range(
            "n_samples",
            params.core.n_samples,
            limits.samples.min,
            limits.samples.max,
        )?;
        params.sampling.cfg_rescale = self.normalize_f32_range(
            "cfg_rescale",
            params.sampling.cfg_rescale,
            limits.cfg_rescale.min,
            limits.cfg_rescale.max,
        )?;
        params.core.size.width =
            self.normalize_image_dimension("size.width", params.core.size.width)?;
        params.core.size.height =
            self.normalize_image_dimension("size.height", params.core.size.height)?;
        params.core.size = self.normalize_canvas_area(params.core.size)?;
        Ok(())
    }

    /// Drops or rejects request groups the selected model does not support.
    fn apply_capability_gates(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        self.gate_characters(params, descriptor)?;
        if params
            .img2img
            .as_ref()
            .is_some_and(Img2ImgRequest::is_infill)
            && !self.require_capability(
                descriptor.support().inpainting.is_some(),
                "img2img.mask",
                "models with an inpainting variant",
                NormalizationChange::Unsupported,
            )?
            && let Some(input) = params.img2img.as_mut()
        {
            input.mask = None;
        }
        self.normalize_references(params, descriptor)?;
        if params.core.quality == QualityPreset::Light
            && !self.require_capability(
                descriptor.prompt().quality.supports(params.core.quality),
                "quality.light",
                "V5 models",
                NormalizationChange::Fallback,
            )?
        {
            params.core.quality = QualityPreset::Standard;
        }
        if params.core.transparent_background
            && !self.require_capability(
                descriptor.prompt().transparent_background,
                "transparent_background",
                "V5 models",
                NormalizationChange::Unsupported,
            )?
        {
            params.core.transparent_background = false;
        }
        self.gate_guidance(&mut params.guidance, descriptor)?;
        Ok(())
    }

    fn gate_characters(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if params
            .characters
            .as_ref()
            .is_some_and(|set| set.enabled_count() > 0)
            && !self.require_capability(
                descriptor.prompt().characters.is_some(),
                "characters",
                "V4 and later models",
                NormalizationChange::Unsupported,
            )?
        {
            params.characters = None;
        }

        if let Some(set) = params.characters.as_mut() {
            let limit = descriptor.prompt().max_characters() as usize;
            if set.enabled_count() > limit && self.reject {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: "characters".to_owned(),
                    value: f64::from(u32::try_from(set.enabled_count()).unwrap_or(u32::MAX)),
                    min: 0.0,
                    max: f64::from(descriptor.prompt().max_characters()),
                }
                .into());
            }
            for (index, character) in set
                .characters
                .iter_mut()
                .enumerate()
                .filter(|(_, character)| character.enabled)
                .skip(limit)
            {
                character.enabled = false;
                self.record(
                    format!("characters[{index}]"),
                    NormalizationChange::CharacterLimit {
                        index,
                        limit: descriptor.prompt().max_characters(),
                    },
                );
            }
        }
        Ok(())
    }

    fn normalize_references(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        let mode = params.mode();
        let Some(references) = params.references.as_mut() else {
            return Ok(());
        };
        let intent = ReferenceIntent {
            character: !references.character_references.is_empty(),
            vibe: references
                .vibe_transfer
                .as_ref()
                .is_some_and(|vibe| !vibe.references.is_empty()),
        };
        let resolution = descriptor.resolve_references(mode, intent);
        if intent.character
            && !self.reference_allowed("character_references", resolution.character)?
        {
            references.character_references.clear();
        }
        if intent.vibe && !self.reference_allowed("vibe_transfer", resolution.vibe)? {
            references.vibe_transfer = None;
        }
        if references.is_empty() {
            params.references = None;
        }
        Ok(())
    }

    // The descriptor owns support and precedence; this boundary only maps decisions to errors
    // or diagnostics, preserving the caller's Reject/Normalize policy.
    fn reference_allowed(
        &mut self,
        field: &str,
        availability: ReferenceAvailability,
    ) -> Result<bool, BridgeError> {
        match availability {
            ReferenceAvailability::Available => Ok(true),
            ReferenceAvailability::NotRequested => Ok(false),
            ReferenceAvailability::Unsupported | ReferenceAvailability::InputNotImplemented => {
                let required = if field == "character_references" {
                    "V4.5 models"
                } else {
                    "V4 and V4.5 models"
                };
                self.require_capability(false, field, required, NormalizationChange::Unsupported)
            }
            ReferenceAvailability::UnsupportedMode if field == "vibe_transfer" => {
                self.reference_conflict("img2img.mask")
            }
            ReferenceAvailability::UnsupportedMode => self.require_capability(
                false,
                "character_references with img2img.mask",
                "models with character-reference inpainting",
                NormalizationChange::Unsupported,
            ),
            ReferenceAvailability::ConflictWithCharacterReference => {
                self.reference_conflict("character_references")
            }
        }
    }

    fn reference_conflict(&mut self, retained_field: &'static str) -> Result<bool, BridgeError> {
        if self.reject {
            return Err(InvalidRequest::UnsupportedFieldCombination {
                left: "vibe_transfer".to_owned(),
                right: retained_field.to_owned(),
            }
            .into());
        }
        self.record(
            "vibe_transfer".to_owned(),
            NormalizationChange::Conflict { retained_field },
        );
        Ok(false)
    }

    fn gate_guidance(
        &mut self,
        guidance: &mut GuidanceParams,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if guidance.variety_boost
            && !self.require_capability(
                descriptor.sampling().variety.is_some(),
                "guidance.variety_boost",
                "V4 and V4.5 models",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.variety_boost = false;
        }

        if guidance.decrisp
            && !self.require_capability(
                descriptor.sampling().dynamic_thresholding,
                "guidance.decrisp",
                "V3 models",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.decrisp = false;
        }

        if (guidance.smea || guidance.smea_dyn)
            && !self.require_capability(
                descriptor.sampling().smea,
                "guidance.smea",
                "V3 models",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.smea = false;
            guidance.smea_dyn = false;
        }

        Ok(())
    }

    /// Returns whether the feature should be kept, honoring `self.reject`.
    fn require_capability(
        &mut self,
        supported: bool,
        feature: &str,
        required_model: &str,
        change: NormalizationChange,
    ) -> Result<bool, BridgeError> {
        if supported {
            return Ok(true);
        }
        if self.reject {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: feature.to_owned(),
                required_model: required_model.to_owned(),
            }
            .into());
        }
        self.record(feature.to_owned(), change);
        Ok(false)
    }

    fn validate_character_inputs(&self, characters: &[Character]) -> Result<(), BridgeError> {
        for (idx, ch) in characters.iter().enumerate().filter(|(_, ch)| ch.enabled) {
            if ch.prompt.trim().is_empty() {
                return Err(InvalidRequest::EmptyField {
                    field: format!("characters[{idx}].prompt"),
                }
                .into());
            }
            if self.reject && !(0.0..=1.0).contains(&ch.position.x) {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: format!("characters[{idx}].position.x"),
                    value: f64::from(ch.position.x),
                    min: 0.0,
                    max: 1.0,
                }
                .into());
            }
            if self.reject && !(0.0..=1.0).contains(&ch.position.y) {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: format!("characters[{idx}].position.y"),
                    value: f64::from(ch.position.y),
                    min: 0.0,
                    max: 1.0,
                }
                .into());
            }
        }

        Ok(())
    }

    fn normalize_character_positions(&mut self, set: &mut CharacterSet) -> Result<(), BridgeError> {
        for (index, ch) in set
            .characters
            .iter_mut()
            .enumerate()
            .filter(|(_, ch)| ch.enabled)
        {
            ch.position.x = self.normalize_f32_range(
                &format!("characters[{index}].position.x"),
                ch.position.x,
                0.0,
                1.0,
            )?;
            ch.position.y = self.normalize_f32_range(
                &format!("characters[{index}].position.y"),
                ch.position.y,
                0.0,
                1.0,
            )?;
        }
        // Preserve automatic placement semantics before snapping to the wire grid.
        let use_coords = resolve_use_coords(Some(set));
        set.use_coords = Some(use_coords);
        for (index, ch) in set
            .characters
            .iter_mut()
            .enumerate()
            .filter(|(_, ch)| ch.enabled)
        {
            for (axis, value) in [("x", &mut ch.position.x), ("y", &mut ch.position.y)] {
                let effective = if use_coords {
                    snap_coord_to_grid(*value)
                } else {
                    0.5
                };
                self.adjusted(
                    &format!("characters[{index}].position.{axis}"),
                    f64::from(*value),
                    f64::from(effective),
                );
                *value = effective;
            }
        }
        Ok(())
    }

    fn normalize_character_references(
        &mut self,
        references: &mut ReferenceSet,
    ) -> Result<(), BridgeError> {
        for (idx, item) in references.character_references.iter_mut().enumerate() {
            item.fidelity = self.normalize_f32_range(
                &format!("character_references[{idx}].fidelity"),
                item.fidelity,
                0.0,
                1.0,
            )?;
            item.strength = self.normalize_f32_range(
                &format!("character_references[{idx}].strength"),
                item.strength,
                0.0,
                1.0,
            )?;
        }

        Ok(())
    }

    fn normalize_img2img(
        &mut self,
        img2img: Option<&mut Img2ImgRequest>,
    ) -> Result<(), BridgeError> {
        let Some(img2img) = img2img else {
            return Ok(());
        };

        let limits = self.constraints;
        img2img.strength = self.normalize_f32_range(
            "img2img.strength",
            img2img.strength,
            limits.img2img_strength.min,
            limits.img2img_strength.max,
        )?;
        img2img.noise = self.normalize_f32_range(
            "img2img.noise",
            img2img.noise,
            limits.img2img_noise.min,
            limits.img2img_noise.max,
        )?;
        Ok(())
    }

    fn normalize_vibe_transfer(
        &mut self,
        vibe: Option<&mut VibeTransferConfig>,
    ) -> Result<(), BridgeError> {
        let Some(vibe) = vibe else {
            return Ok(());
        };

        vibe.strength =
            self.normalize_f32_range("vibe_transfer.strength", vibe.strength, 0.0, 1.0)?;
        if vibe.references.is_empty() {
            return Err(InvalidRequest::EmptyField {
                field: "vibe_transfer.references".to_owned(),
            }
            .into());
        }

        for (idx, reference) in vibe.references.iter_mut().enumerate() {
            if reference.vibe_data_cache.trim().is_empty() {
                return Err(InvalidRequest::EmptyField {
                    field: format!("vibe_transfer.references[{idx}].vibe_data_cache"),
                }
                .into());
            }
            reference.strength = self.normalize_f32_range(
                &format!("vibe_transfer.references[{idx}].strength"),
                reference.strength,
                0.0,
                1.0,
            )?;
        }

        Ok(())
    }

    fn normalize_u32_range(
        &mut self,
        field_name: &str,
        value: u32,
        min: u32,
        max: u32,
    ) -> Result<u32, BridgeError> {
        if self.reject && !(min..=max).contains(&value) {
            return Err(InvalidRequest::NumericOutOfRange {
                field: field_name.to_owned(),
                value: f64::from(value),
                min: f64::from(min),
                max: f64::from(max),
            }
            .into());
        }
        let effective = value.clamp(min, max);
        self.adjusted(field_name, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    fn normalize_f32_range(
        &mut self,
        field_name: &str,
        value: f32,
        min: f32,
        max: f32,
    ) -> Result<f32, BridgeError> {
        if !value.is_finite() {
            return Err(InvalidRequest::NonFiniteNumber {
                field: field_name.to_owned(),
                value: f64::from(value),
            }
            .into());
        }
        if self.reject && !(min..=max).contains(&value) {
            return Err(InvalidRequest::NumericOutOfRange {
                field: field_name.to_owned(),
                value: f64::from(value),
                min: f64::from(min),
                max: f64::from(max),
            }
            .into());
        }
        let effective = value.clamp(min, max);
        self.adjusted(field_name, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    fn normalize_image_dimension(
        &mut self,
        field_name: &str,
        value: u32,
    ) -> Result<u32, BridgeError> {
        let limits = self.constraints.canvas;
        if self.reject
            && (!(limits.dimension.min..=limits.dimension.max).contains(&value)
                || !value.is_multiple_of(limits.multiple))
        {
            return Err(InvalidRequest::InvalidImageDimension {
                field: field_name.to_owned(),
                value,
                min: limits.dimension.min,
                max: limits.dimension.max,
                multiple_of: limits.multiple,
            }
            .into());
        }

        let clamped = value.clamp(limits.dimension.min, limits.dimension.max);
        let snapped = ((clamped + (limits.multiple / 2)) / limits.multiple) * limits.multiple;
        let effective = snapped.clamp(limits.dimension.min, limits.dimension.max);
        self.adjusted(field_name, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    /// Shrinks a canvas that exceeds the API pixel budget, keeping the 64 pixel grid.
    fn normalize_canvas_area(&mut self, size: ImageSize) -> Result<ImageSize, BridgeError> {
        let limits = self.constraints.canvas;
        if size.pixels() <= limits.max_pixels {
            return Ok(size);
        }
        if self.reject {
            return Err(InvalidRequest::InvalidImageDimension {
                field: "size".to_owned(),
                value: size.width,
                min: limits.dimension.min,
                max: limits.dimension.max,
                multiple_of: limits.multiple,
            }
            .into());
        }

        let mut shrunk = size;
        while shrunk.pixels() > limits.max_pixels {
            if (shrunk.width >= shrunk.height && shrunk.width > limits.dimension.min)
                || shrunk.height == limits.dimension.min
            {
                shrunk.width -= limits.multiple;
            } else {
                shrunk.height -= limits.multiple;
            }
        }
        self.adjusted("size.width", f64::from(size.width), f64::from(shrunk.width));
        self.adjusted(
            "size.height",
            f64::from(size.height),
            f64::from(shrunk.height),
        );
        Ok(shrunk)
    }
}

fn resolve_use_coords(characters: Option<&CharacterSet>) -> bool {
    let Some(characters) = characters else {
        return false;
    };
    if characters.enabled_count() == 0 {
        return false;
    }
    if let Some(explicit) = characters.use_coords {
        return explicit;
    }
    characters.enabled().any(|character| {
        !is_center_position(character.position.x) || !is_center_position(character.position.y)
    })
}

fn validate_prompt(params: &GenerateImageRequest) -> Result<(), BridgeError> {
    if params.core.prompt.trim().is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: "prompt".to_owned(),
        }
        .into());
    }
    Ok(())
}
