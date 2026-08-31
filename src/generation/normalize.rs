use super::{
    NormalizationChange, NormalizationDiagnostic, NormalizationPolicy, NormalizedGeneration,
};
use crate::{
    BridgeError, Character, CharacterSet, GenerateImageRequest, GuidanceParams, ImageSize,
    Img2ImgRequest, InvalidRequest, ModelCapabilities, QualityPreset, ReferenceSet,
    VibeTransferConfig,
    internal::request_helpers::{is_center_position, snap_coord_to_grid},
};

/// Largest canvas area accepted by `NovelAI` image generation.
const MAX_IMAGE_PIXELS: u64 = 3_145_728;
const IMAGE_DIMENSION_MIN: u32 = 64;
const IMAGE_DIMENSION_MAX: u32 = 1600;
const IMAGE_DIMENSION_MULTIPLE: u32 = 64;

pub(super) fn normalize(
    mut request: GenerateImageRequest,
    policy: NormalizationPolicy,
) -> Result<NormalizedGeneration, BridgeError> {
    request.strict_mode = policy == NormalizationPolicy::Reject;
    let mut normalizer = Normalizer {
        reject: request.strict_mode,
        diagnostics: Vec::new(),
    };
    validate_prompt(&request)?;
    let caps = request.capabilities();
    normalizer.normalize_base_fields(&mut request)?;
    normalizer.apply_capability_gates(&mut request, &caps)?;
    normalizer.validate_feature_combinations(&mut request)?;
    if let Some(characters) = request.characters.as_mut() {
        normalizer.validate_character_inputs(&characters.characters)?;
        normalizer.normalize_character_positions(characters)?;
    }
    if let Some(references) = request.references.as_mut() {
        normalizer.normalize_character_references(references)?;
        normalizer.normalize_vibe_transfer(references.vibe_transfer.as_mut())?;
    }
    normalizer.normalize_img2img(request.img2img.as_mut())?;
    let schedule = request.sampling.noise_schedule.resolve_for(&caps);
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
        params.sampling.steps = self.normalize_u32_range("steps", params.sampling.steps, 1, 50)?;
        params.sampling.scale =
            self.normalize_f32_range("scale", params.sampling.scale, 0.0, 10.0)?;
        params.core.n_samples =
            self.normalize_u32_range("n_samples", params.core.n_samples, 1, 4)?;
        params.sampling.cfg_rescale =
            self.normalize_f32_range("cfg_rescale", params.sampling.cfg_rescale, 0.0, 1.0)?;
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
        capabilities: &ModelCapabilities,
    ) -> Result<(), BridgeError> {
        self.gate_characters(params, capabilities)?;

        let has_inpaint_mask = params
            .img2img
            .as_ref()
            .is_some_and(Img2ImgRequest::is_infill)
            && capabilities.supports_inpainting;
        if let Some(references) = params.references.as_mut() {
            self.gate_character_references(references, capabilities, has_inpaint_mask)?;

            if references
                .vibe_transfer
                .as_ref()
                .is_some_and(|vibe| !vibe.references.is_empty())
                && !self.require_capability(
                    capabilities.supports_encoded_vibe,
                    "vibe_transfer",
                    "V4 and V4.5 models",
                    NormalizationChange::Unsupported,
                )?
            {
                references.vibe_transfer = None;
            }

            if references.is_empty() {
                params.references = None;
            }
        }

        if params
            .img2img
            .as_ref()
            .is_some_and(Img2ImgRequest::is_infill)
            && !self.require_capability(
                capabilities.supports_inpainting,
                "img2img.mask",
                "models with an inpainting variant",
                NormalizationChange::Unsupported,
            )?
            && let Some(img2img) = params.img2img.as_mut()
        {
            img2img.mask = None;
        }

        if params.core.quality == QualityPreset::Light
            && !self.require_capability(
                capabilities.supports_light_quality_preset,
                "quality.light",
                "V5 models",
                NormalizationChange::Fallback,
            )?
        {
            params.core.quality = QualityPreset::Standard;
        }

        if params.core.transparent_background
            && !self.require_capability(
                capabilities.supports_transparent_background,
                "transparent_background",
                "V5 models",
                NormalizationChange::Unsupported,
            )?
        {
            params.core.transparent_background = false;
        }

        self.gate_guidance(&mut params.guidance, capabilities)?;
        Ok(())
    }

    fn gate_characters(
        &mut self,
        params: &mut GenerateImageRequest,
        capabilities: &ModelCapabilities,
    ) -> Result<(), BridgeError> {
        if params
            .characters
            .as_ref()
            .is_some_and(|set| set.enabled_count() > 0)
            && !self.require_capability(
                capabilities.supports_characters(),
                "characters",
                "V4 and later models",
                NormalizationChange::Unsupported,
            )?
        {
            params.characters = None;
        }

        if let Some(set) = params.characters.as_mut() {
            let limit = capabilities.max_characters as usize;
            if set.enabled_count() > limit && self.reject {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: "characters".to_owned(),
                    value: f64::from(u32::try_from(set.enabled_count()).unwrap_or(u32::MAX)),
                    min: 0.0,
                    max: f64::from(capabilities.max_characters),
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
                        limit: capabilities.max_characters,
                    },
                );
            }
        }
        Ok(())
    }

    fn gate_character_references(
        &mut self,
        references: &mut ReferenceSet,
        capabilities: &ModelCapabilities,
        has_inpaint_mask: bool,
    ) -> Result<(), BridgeError> {
        if !references.character_references.is_empty()
            && !self.require_capability(
                capabilities.supports_character_reference,
                "character_references",
                "V4.5 models",
                NormalizationChange::Unsupported,
            )?
        {
            references.character_references.clear();
        }

        if !references.character_references.is_empty()
            && has_inpaint_mask
            && !self.require_capability(
                capabilities.supports_character_reference_inpainting,
                "character_references with img2img.mask",
                "models with character-reference inpainting",
                NormalizationChange::Unsupported,
            )?
        {
            references.character_references.clear();
        }
        Ok(())
    }

    fn gate_guidance(
        &mut self,
        guidance: &mut GuidanceParams,
        capabilities: &ModelCapabilities,
    ) -> Result<(), BridgeError> {
        if guidance.variety_boost
            && !self.require_capability(
                capabilities.supports_variety_boost,
                "guidance.variety_boost",
                "V4 and V4.5 models",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.variety_boost = false;
        }

        if guidance.decrisp
            && !self.require_capability(
                capabilities.supports_dynamic_thresholding,
                "guidance.decrisp",
                "V3 models",
                NormalizationChange::Unsupported,
            )?
        {
            guidance.decrisp = false;
        }

        if (guidance.smea || guidance.smea_dyn)
            && !self.require_capability(
                capabilities.supports_smea,
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

    /// Rejects or drops field combinations `NovelAI` does not accept together.
    fn validate_feature_combinations(
        &mut self,
        params: &mut GenerateImageRequest,
    ) -> Result<(), BridgeError> {
        let Some(references) = params.references.as_mut() else {
            return Ok(());
        };

        let has_vibe = references
            .vibe_transfer
            .as_ref()
            .is_some_and(|vibe| !vibe.references.is_empty());

        if has_vibe && !references.character_references.is_empty() {
            if self.reject {
                return Err(InvalidRequest::UnsupportedFieldCombination {
                    left: "vibe_transfer".to_owned(),
                    right: "character_references".to_owned(),
                }
                .into());
            }
            self.record(
                "vibe_transfer".to_owned(),
                NormalizationChange::Conflict {
                    retained_field: "character_references",
                },
            );
            references.vibe_transfer = None;
        } else if has_vibe
            && params
                .img2img
                .as_ref()
                .is_some_and(Img2ImgRequest::is_infill)
        {
            // NovelAI returns a server error when an infill request also carries a vibe payload.
            if self.reject {
                return Err(InvalidRequest::UnsupportedFieldCombination {
                    left: "vibe_transfer".to_owned(),
                    right: "img2img.mask".to_owned(),
                }
                .into());
            }
            self.record(
                "vibe_transfer".to_owned(),
                NormalizationChange::Conflict {
                    retained_field: "img2img.mask",
                },
            );
            references.vibe_transfer = None;
        }

        if references.is_empty() {
            params.references = None;
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

        img2img.strength =
            self.normalize_f32_range("img2img.strength", img2img.strength, 0.01, 0.99)?;
        img2img.noise = self.normalize_f32_range("img2img.noise", img2img.noise, 0.0, 0.99)?;
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
        if self.reject
            && (!(IMAGE_DIMENSION_MIN..=IMAGE_DIMENSION_MAX).contains(&value)
                || !value.is_multiple_of(IMAGE_DIMENSION_MULTIPLE))
        {
            return Err(InvalidRequest::InvalidImageDimension {
                field: field_name.to_owned(),
                value,
                min: IMAGE_DIMENSION_MIN,
                max: IMAGE_DIMENSION_MAX,
                multiple_of: IMAGE_DIMENSION_MULTIPLE,
            }
            .into());
        }

        let clamped = value.clamp(IMAGE_DIMENSION_MIN, IMAGE_DIMENSION_MAX);
        let snapped = ((clamped + (IMAGE_DIMENSION_MULTIPLE / 2)) / IMAGE_DIMENSION_MULTIPLE)
            * IMAGE_DIMENSION_MULTIPLE;
        let effective = snapped.clamp(IMAGE_DIMENSION_MIN, IMAGE_DIMENSION_MAX);
        self.adjusted(field_name, f64::from(value), f64::from(effective));
        Ok(effective)
    }

    /// Shrinks a canvas that exceeds the API pixel budget, keeping the 64 pixel grid.
    fn normalize_canvas_area(&mut self, size: ImageSize) -> Result<ImageSize, BridgeError> {
        if size.pixels() <= MAX_IMAGE_PIXELS {
            return Ok(size);
        }
        if self.reject {
            return Err(InvalidRequest::InvalidImageDimension {
                field: "size".to_owned(),
                value: size.width,
                min: IMAGE_DIMENSION_MIN,
                max: IMAGE_DIMENSION_MAX,
                multiple_of: IMAGE_DIMENSION_MULTIPLE,
            }
            .into());
        }

        let mut shrunk = size;
        while shrunk.pixels() > MAX_IMAGE_PIXELS {
            if (shrunk.width >= shrunk.height && shrunk.width > IMAGE_DIMENSION_MIN)
                || shrunk.height == IMAGE_DIMENSION_MIN
            {
                shrunk.width -= IMAGE_DIMENSION_MULTIPLE;
            } else {
                shrunk.height -= IMAGE_DIMENSION_MULTIPLE;
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
