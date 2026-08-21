use crate::{
    BridgeError, Character, CharacterSet, GenerateImageRequest, GenerateImageResult,
    GenerateImageStream, GenerateImageStreamRequest, GuidanceParams, ImageSize, Img2ImgRequest,
    InvalidRequest, ModelCapabilities, ReferenceSet, Transport, VibeTransferConfig,
    internal::{
        request_helpers::{
            build_v4_character_captions, convert_character_prompts, convert_character_references,
            default_seed, extract_images, fit_mask_image, fit_source_image, is_center_position,
        },
        wire::{
            CharacterPrompt, ImageGenerationRequestBody, ImageParameters, ImageStreamParameters,
            StreamImageGenerationRequestBody, V4Caption, V4ConditionInput,
        },
    },
};

use super::Client;

/// Largest canvas area accepted by `NovelAI` image generation.
const MAX_IMAGE_PIXELS: u64 = 3_145_728;
const IMAGE_DIMENSION_MIN: u32 = 64;
const IMAGE_DIMENSION_MAX: u32 = 1600;
const IMAGE_DIMENSION_MULTIPLE: u32 = 64;

/// Wire `action` selected from the request shape.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum GenerationAction {
    Generate,
    Img2Img,
    Infill,
}

impl GenerationAction {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Img2Img => "img2img",
            Self::Infill => "infill",
        }
    }

    const fn of(params: &GenerateImageRequest) -> Self {
        match params.img2img.as_ref() {
            None => Self::Generate,
            Some(img2img) if img2img.is_infill() => Self::Infill,
            Some(_) => Self::Img2Img,
        }
    }
}

struct VibeWireParts {
    images: Vec<String>,
    strengths: Vec<f32>,
    information_extracted: Vec<f32>,
    strength: f32,
}

struct PromptWireParts {
    prompt_field: Option<String>,
    legacy_uc: Option<String>,
    negative_prompt: String,
    v4_prompt: Option<V4ConditionInput>,
    v4_negative_prompt: Option<V4ConditionInput>,
    character_prompts: Option<Vec<CharacterPrompt>>,
}

#[derive(Default)]
struct Img2ImgWireParts {
    image: Option<String>,
    mask: Option<String>,
    strength: Option<f32>,
    noise: Option<f32>,
    extra_noise_seed: Option<i64>,
    color_correct: Option<bool>,
    request_type: Option<String>,
    inpaint_strength: Option<f32>,
}

impl<T: Transport> Client<T> {
    /// Generates images from a request and returns the decoded image payloads.
    ///
    /// The client validates request ranges, model capabilities and payload shape locally, and
    /// fits image inputs to the requested canvas, but it intentionally does not parse or reject
    /// prompt syntax before sending the request.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when request validation, image normalization, transport, or response
    /// decoding fails.
    pub async fn generate(
        &self,
        params: GenerateImageRequest,
    ) -> Result<GenerateImageResult, BridgeError> {
        let params = Self::normalize_generate_request(params)?;
        let resolved_seed = Self::resolve_seed(params.core.seed);
        let request = Self::build_generate_request(&params, resolved_seed)?;
        let endpoint = self.image_endpoint("/ai/generate-image");

        let observation = self
            .begin_generation_observation(&params, crate::ObservedOperation::GenerateImage)
            .await;
        let payload = match self.transport.post_bytes(endpoint.as_str(), &request).await {
            Ok(payload) => payload,
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                return Err(error);
            }
        };

        let images = extract_images(payload);
        let image_count = u32::try_from(images.len()).unwrap_or(u32::MAX);
        self.finish_observation_succeeded(observation, image_count)
            .await;

        Ok(GenerateImageResult {
            resolved_seed,
            images,
        })
    }

    /// Starts a streaming image generation request and returns parsed SSE chunks.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when request validation, image normalization, transport, or stream
    /// setup fails.
    pub async fn generate_stream(
        &self,
        params: GenerateImageStreamRequest,
    ) -> Result<GenerateImageStream, BridgeError> {
        let params = GenerateImageStreamRequest {
            base: Self::normalize_generate_request(params.base)?,
            stream: params.stream,
        };
        let resolved_seed = Self::resolve_seed(params.base.core.seed);
        let request = Self::build_stream_request(&params, resolved_seed)?;
        let endpoint = self.image_endpoint("/ai/generate-image-stream");

        let observation = self
            .begin_generation_observation(
                &params.base,
                crate::ObservedOperation::GenerateImageStream,
            )
            .await;
        match self.transport.post_sse(endpoint.as_str(), &request).await {
            Ok(stream) => {
                self.finish_observation_succeeded(observation, 0).await;
                Ok(GenerateImageStream::new(resolved_seed, stream))
            }
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                Err(error)
            }
        }
    }

    fn normalize_generate_request(
        mut params: GenerateImageRequest,
    ) -> Result<GenerateImageRequest, BridgeError> {
        validate_prompt(&params)?;
        let strict_mode = params.strict_mode;
        let capabilities = params.core.model.capabilities();

        normalize_base_fields(&mut params, strict_mode)?;
        apply_capability_gates(&mut params, &capabilities, strict_mode)?;
        validate_feature_combinations(&mut params, strict_mode)?;

        if let Some(characters) = params.characters.as_mut() {
            validate_character_inputs(&characters.characters, strict_mode)?;
            normalize_character_positions(&mut characters.characters)?;
        }
        if let Some(references) = params.references.as_mut() {
            normalize_character_references(references, strict_mode)?;
            normalize_vibe_transfer(references.vibe_transfer.as_mut(), strict_mode)?;
        }
        normalize_img2img(params.img2img.as_mut(), strict_mode)?;

        Ok(params)
    }

    fn build_generate_request(
        params: &GenerateImageRequest,
        resolved_seed: i64,
    ) -> Result<ImageGenerationRequestBody, BridgeError> {
        let action = GenerationAction::of(params);
        let model = if action == GenerationAction::Infill {
            params.core.model.inpainting_id()
        } else {
            params.core.model.as_str()
        };
        let image_parameters = Self::build_image_parameters(params, action, resolved_seed)?;

        Ok(ImageGenerationRequestBody {
            action: action.as_str().to_owned(),
            input: params.core.prompt.clone(),
            model: model.to_owned(),
            use_new_shared_trial: true,
            parameters: image_parameters,
        })
    }

    fn build_stream_request(
        params: &GenerateImageStreamRequest,
        resolved_seed: i64,
    ) -> Result<StreamImageGenerationRequestBody, BridgeError> {
        let base = Self::build_generate_request(&params.base, resolved_seed)?;
        Ok(StreamImageGenerationRequestBody {
            action: base.action,
            input: base.input,
            model: base.model,
            use_new_shared_trial: base.use_new_shared_trial,
            parameters: ImageStreamParameters {
                base: base.parameters,
                stream: params.stream.as_str().to_owned(),
            },
        })
    }

    #[allow(clippy::too_many_lines)]
    fn build_image_parameters(
        params: &GenerateImageRequest,
        action: GenerationAction,
        resolved_seed: i64,
    ) -> Result<ImageParameters, BridgeError> {
        let capabilities = params.core.model.capabilities();
        let uses_v4 = capabilities.uses_v4_prompt();
        let resolved_use_coords = resolve_use_coords(params.characters.as_ref());
        let prompt = build_prompt_wire_parts(params, &capabilities, resolved_use_coords);
        let img2img = build_img2img_wire_parts(params, action, resolved_seed)?;

        let references = params.references.as_ref();
        let vibe = references
            .and_then(|set| set.vibe_transfer.as_ref())
            .map(Self::encode_vibe_references)
            .transpose()?;
        let character_references = references
            .map(|set| set.character_references.as_slice())
            .filter(|refs| !refs.is_empty())
            .map(convert_character_references)
            .transpose()?;

        let variety_sigma = params
            .guidance
            .variety_boost
            .then(|| capabilities.variety_sigma(params.core.size.width, params.core.size.height))
            .flatten();

        Ok(ImageParameters {
            params_version: Some(capabilities.params_version),
            width: Some(params.core.size.width),
            height: Some(params.core.size.height),
            steps: Some(params.sampling.steps),
            scale: Some(params.sampling.scale),
            sampler: Some(params.sampling.sampler.as_str().to_owned()),
            seed: Some(resolved_seed),
            n_samples: Some(params.core.n_samples),
            prompt: prompt.prompt_field,
            negative_prompt: Some(prompt.negative_prompt),
            uc: prompt.legacy_uc,
            uc_preset: Some(params.core.uc_preset.as_api_value()),
            quality_toggle: Some(params.core.quality),
            v4_prompt: prompt.v4_prompt,
            v4_negative_prompt: prompt.v4_negative_prompt,
            cfg_rescale: Some(params.sampling.cfg_rescale),
            noise_schedule: Some(
                params
                    .sampling
                    .noise_schedule
                    .resolve_for(&capabilities)
                    .as_str()
                    .to_owned(),
            ),
            skip_cfg_above_sigma: variety_sigma,
            request_type: img2img.request_type,
            image: img2img.image,
            strength: img2img.strength,
            noise: img2img.noise,
            extra_noise_seed: img2img.extra_noise_seed,
            color_correct: img2img.color_correct,
            mask: img2img.mask,
            reference_image_multiple: vibe.as_ref().map(|value| value.images.clone()),
            reference_strength_multiple: vibe.as_ref().map(|value| value.strengths.clone()),
            // TODO(verify): a 2026 web capture omits this field because the value is baked into
            // the encoded vibe payload. Kept until a live request confirms it is optional.
            reference_information_extracted_multiple: vibe
                .as_ref()
                .map(|value| value.information_extracted.clone()),
            normalize_reference_strength_multiple: vibe.as_ref().map(|_| true),
            controlnet_strength: Some(vibe.as_ref().map_or(1.0, |value| value.strength)),
            director_reference_images: character_references
                .as_ref()
                .map(|value| value.images.clone()),
            director_reference_descriptions: character_references
                .as_ref()
                .map(|value| value.descriptions.clone()),
            director_reference_information_extracted: character_references
                .as_ref()
                .map(|value| value.information_extracted.clone()),
            director_reference_secondary_strength_values: character_references
                .as_ref()
                .map(|value| value.secondary_strength_values.clone()),
            director_reference_strength_values: character_references
                .as_ref()
                .map(|value| value.strength_values.clone()),
            character_prompts: prompt.character_prompts,
            legacy: Some(false),
            legacy_uc: capabilities.uses_v5_extensions.then_some(false),
            legacy_v3_extend: Some(false),
            deliberate_euler_ancestral_bug: Some(false),
            prefer_brownian: Some(true),
            auto_smea: Some(false),
            sm: capabilities.supports_smea.then_some(params.guidance.smea),
            sm_dyn: capabilities
                .supports_smea
                .then_some(params.guidance.smea_dyn),
            dynamic_thresholding: capabilities
                .supports_dynamic_thresholding
                .then_some(params.guidance.decrisp),
            add_original_image: Some(
                params
                    .img2img
                    .as_ref()
                    .is_none_or(|img2img| img2img.add_original_image),
            ),
            inpaint_img2_img_strength: Some(img2img.inpaint_strength.unwrap_or(1.0)),
            tag_hint_transparent_background: capabilities.uses_v5_extensions.then_some(true),
            use_coords: uses_v4.then_some(resolved_use_coords),
            image_format: params
                .output
                .image_format
                .map(|format| format.as_str().to_owned()),
        })
    }

    fn resolve_seed(seed: i64) -> i64 {
        if seed == 0 { default_seed() } else { seed }
    }

    fn encode_vibe_references(
        vibe: &VibeTransferConfig,
    ) -> Result<VibeWireParts, BridgeError> {
        let mut images = Vec::with_capacity(vibe.references.len());
        let mut strengths = Vec::with_capacity(vibe.references.len());
        let mut information_extracted = Vec::with_capacity(vibe.references.len());

        for reference in &vibe.references {
            images.push(Self::resolve_vibe_data_cache(&reference.vibe_data_cache)?);
            strengths.push(reference.strength);
            information_extracted.push(reference.info_extracted);
        }

        Ok(VibeWireParts {
            images,
            strengths,
            information_extracted,
            strength: vibe.strength,
        })
    }
}

pub(super) fn resolve_use_coords(characters: Option<&CharacterSet>) -> bool {
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

fn normalize_base_fields(
    params: &mut GenerateImageRequest,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    params.sampling.steps = normalize_u32_range("steps", params.sampling.steps, 1, 50, strict_mode)?;
    params.sampling.scale =
        normalize_f32_range("scale", params.sampling.scale, 0.0, 10.0, strict_mode)?;
    params.core.n_samples =
        normalize_u32_range("n_samples", params.core.n_samples, 1, 4, strict_mode)?;
    params.sampling.cfg_rescale = normalize_f32_range(
        "cfg_rescale",
        params.sampling.cfg_rescale,
        0.0,
        1.0,
        strict_mode,
    )?;
    params.core.size.width =
        normalize_image_dimension("size.width", params.core.size.width, strict_mode)?;
    params.core.size.height =
        normalize_image_dimension("size.height", params.core.size.height, strict_mode)?;
    params.core.size = normalize_canvas_area(params.core.size, strict_mode)?;
    Ok(())
}

/// Drops or rejects request groups the selected model does not support.
fn apply_capability_gates(
    params: &mut GenerateImageRequest,
    capabilities: &ModelCapabilities,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    if params
        .characters
        .as_ref()
        .is_some_and(|set| !set.characters.is_empty())
        && !require_capability(
            capabilities.supports_characters(),
            "characters",
            "V4 and later models",
            strict_mode,
        )?
    {
        params.characters = None;
    }

    if let Some(set) = params.characters.as_ref()
        && set.enabled_count() > capabilities.max_characters as usize
        && strict_mode
    {
        return Err(InvalidRequest::NumericOutOfRange {
            field: "characters".to_owned(),
            value: f64::from(u32::try_from(set.enabled_count()).unwrap_or(u32::MAX)),
            min: 0.0,
            max: f64::from(capabilities.max_characters),
        }
        .into());
    }

    if let Some(references) = params.references.as_mut() {
        if !references.character_references.is_empty()
            && !require_capability(
                capabilities.supports_character_reference,
                "character_references",
                "V4.5 models",
                strict_mode,
            )?
        {
            references.character_references.clear();
        }

        if references
            .vibe_transfer
            .as_ref()
            .is_some_and(|vibe| !vibe.references.is_empty())
            && !require_capability(
                capabilities.supports_encoded_vibe,
                "vibe_transfer",
                "V4 and V4.5 models",
                strict_mode,
            )?
        {
            references.vibe_transfer = None;
        }

        if references.is_empty() {
            params.references = None;
        }
    }

    if params.img2img.as_ref().is_some_and(Img2ImgRequest::is_infill)
        && !require_capability(
            capabilities.supports_inpainting,
            "img2img.mask",
            "models with an inpainting variant",
            strict_mode,
        )?
        && let Some(img2img) = params.img2img.as_mut()
    {
        img2img.mask = None;
    }

    gate_guidance(&mut params.guidance, capabilities, strict_mode)?;
    Ok(())
}

fn gate_guidance(
    guidance: &mut GuidanceParams,
    capabilities: &ModelCapabilities,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    if guidance.variety_boost
        && !require_capability(
            capabilities.supports_variety_boost,
            "guidance.variety_boost",
            "V4 and V4.5 models",
            strict_mode,
        )?
    {
        guidance.variety_boost = false;
    }

    if guidance.decrisp
        && !require_capability(
            capabilities.supports_dynamic_thresholding,
            "guidance.decrisp",
            "V3 models",
            strict_mode,
        )?
    {
        guidance.decrisp = false;
    }

    if (guidance.smea || guidance.smea_dyn)
        && !require_capability(
            capabilities.supports_smea,
            "guidance.smea",
            "V3 models",
            strict_mode,
        )?
    {
        guidance.smea = false;
        guidance.smea_dyn = false;
    }

    Ok(())
}

/// Rejects or drops field combinations `NovelAI` does not accept together.
fn validate_feature_combinations(
    params: &mut GenerateImageRequest,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(references) = params.references.as_mut() else {
        return Ok(());
    };

    let has_vibe = references
        .vibe_transfer
        .as_ref()
        .is_some_and(|vibe| !vibe.references.is_empty());

    if has_vibe && !references.character_references.is_empty() {
        if strict_mode {
            return Err(InvalidRequest::UnsupportedFieldCombination {
                left: "vibe_transfer".to_owned(),
                right: "character_references".to_owned(),
            }
            .into());
        }
        log::warn!("dropping vibe_transfer: not supported together with character_references");
        references.vibe_transfer = None;
    } else if has_vibe && params.img2img.as_ref().is_some_and(Img2ImgRequest::is_infill) {
        // NovelAI returns a server error when an infill request also carries a vibe payload.
        if strict_mode {
            return Err(InvalidRequest::UnsupportedFieldCombination {
                left: "vibe_transfer".to_owned(),
                right: "img2img.mask".to_owned(),
            }
            .into());
        }
        log::warn!("dropping vibe_transfer: not supported by infill requests");
        references.vibe_transfer = None;
    }

    if references.is_empty() {
        params.references = None;
    }
    Ok(())
}

/// Returns whether the feature should be kept, honoring `strict_mode`.
fn require_capability(
    supported: bool,
    feature: &str,
    required_model: &str,
    strict_mode: bool,
) -> Result<bool, BridgeError> {
    if supported {
        return Ok(true);
    }
    if strict_mode {
        return Err(InvalidRequest::UnsupportedModelFeature {
            feature: feature.to_owned(),
            required_model: required_model.to_owned(),
        }
        .into());
    }
    log::warn!("dropping {feature}: only supported on {required_model}");
    Ok(false)
}

fn validate_character_inputs(
    characters: &[Character],
    strict_mode: bool,
) -> Result<(), BridgeError> {
    for (idx, ch) in characters.iter().enumerate() {
        if ch.prompt.trim().is_empty() {
            return Err(InvalidRequest::EmptyField {
                field: format!("characters[{idx}].prompt"),
            }
            .into());
        }
        if strict_mode && !(0.0..=1.0).contains(&ch.position.x) {
            return Err(InvalidRequest::NumericOutOfRange {
                field: format!("characters[{idx}].position.x"),
                value: f64::from(ch.position.x),
                min: 0.0,
                max: 1.0,
            }
            .into());
        }
        if strict_mode && !(0.0..=1.0).contains(&ch.position.y) {
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

fn normalize_character_positions(characters: &mut [Character]) -> Result<(), BridgeError> {
    for ch in characters {
        ch.position.x =
            normalize_f32_range("character.position.x", ch.position.x, 0.0, 1.0, false)?;
        ch.position.y =
            normalize_f32_range("character.position.y", ch.position.y, 0.0, 1.0, false)?;
    }

    Ok(())
}

fn normalize_character_references(
    references: &mut ReferenceSet,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    for (idx, item) in references.character_references.iter_mut().enumerate() {
        item.fidelity = normalize_f32_range(
            &format!("character_references[{idx}].fidelity"),
            item.fidelity,
            0.0,
            1.0,
            strict_mode,
        )?;
        item.strength = normalize_f32_range(
            &format!("character_references[{idx}].strength"),
            item.strength,
            0.0,
            1.0,
            strict_mode,
        )?;
    }

    Ok(())
}

fn normalize_img2img(
    img2img: Option<&mut Img2ImgRequest>,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(img2img) = img2img else {
        return Ok(());
    };

    img2img.strength = normalize_f32_range(
        "img2img.strength",
        img2img.strength,
        0.01,
        0.99,
        strict_mode,
    )?;
    img2img.noise = normalize_f32_range("img2img.noise", img2img.noise, 0.0, 0.99, strict_mode)?;
    Ok(())
}

fn normalize_vibe_transfer(
    vibe: Option<&mut VibeTransferConfig>,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(vibe) = vibe else {
        return Ok(());
    };

    vibe.strength = normalize_f32_range(
        "vibe_transfer.strength",
        vibe.strength,
        0.0,
        1.0,
        strict_mode,
    )?;
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
        reference.info_extracted = normalize_f32_range(
            &format!("vibe_transfer.references[{idx}].info_extracted"),
            reference.info_extracted,
            0.01,
            1.0,
            strict_mode,
        )?;
        reference.strength = normalize_f32_range(
            &format!("vibe_transfer.references[{idx}].strength"),
            reference.strength,
            0.0,
            1.0,
            strict_mode,
        )?;
    }

    Ok(())
}

fn build_prompt_wire_parts(
    params: &GenerateImageRequest,
    capabilities: &ModelCapabilities,
    resolved_use_coords: bool,
) -> PromptWireParts {
    let processed_prompt = params.processed_prompt();
    let negative_prompt = params.processed_negative_prompt();
    let characters = params
        .characters
        .as_ref()
        .map(|set| set.characters.as_slice());

    if capabilities.uses_v4_prompt() {
        let char_prompts = build_v4_character_captions(characters, false, resolved_use_coords);
        let char_negative_prompts =
            build_v4_character_captions(characters, true, resolved_use_coords);

        return PromptWireParts {
            prompt_field: None,
            legacy_uc: None,
            v4_prompt: Some(V4ConditionInput {
                caption: Some(V4Caption {
                    base_caption: Some(processed_prompt),
                    char_captions: Some(char_prompts),
                }),
                legacy_uc: None,
                use_coords: Some(resolved_use_coords),
                use_order: Some(true),
            }),
            v4_negative_prompt: Some(V4ConditionInput {
                caption: Some(V4Caption {
                    base_caption: Some(negative_prompt.clone()),
                    char_captions: Some(char_negative_prompts),
                }),
                legacy_uc: None,
                use_coords: None,
                use_order: None,
            }),
            negative_prompt,
            character_prompts: Some(
                characters
                    .map(|characters| convert_character_prompts(characters, resolved_use_coords))
                    .unwrap_or_default(),
            ),
        };
    }

    PromptWireParts {
        prompt_field: Some(processed_prompt),
        legacy_uc: Some(negative_prompt.clone()),
        negative_prompt,
        v4_prompt: None,
        v4_negative_prompt: None,
        character_prompts: None,
    }
}

fn build_img2img_wire_parts(
    params: &GenerateImageRequest,
    action: GenerationAction,
    resolved_seed: i64,
) -> Result<Img2ImgWireParts, BridgeError> {
    let Some(img2img) = params.img2img.as_ref() else {
        return Ok(Img2ImgWireParts::default());
    };

    let image = fit_source_image("img2img.image", &img2img.image, params.core.size)?;

    if action == GenerationAction::Infill {
        let mask = img2img
            .mask
            .as_ref()
            .map(|mask| fit_mask_image("img2img.mask", mask, params.core.size))
            .transpose()?;

        return Ok(Img2ImgWireParts {
            image: Some(image),
            mask,
            noise: Some(img2img.noise),
            request_type: Some("NativeInfillingRequest".to_owned()),
            inpaint_strength: Some(img2img.strength),
            ..Img2ImgWireParts::default()
        });
    }

    Ok(Img2ImgWireParts {
        image: Some(image),
        strength: Some(img2img.strength),
        noise: Some(img2img.noise),
        extra_noise_seed: Some(img2img.extra_noise_seed.unwrap_or(resolved_seed)),
        color_correct: Some(img2img.color_correct),
        ..Img2ImgWireParts::default()
    })
}

fn normalize_u32_range(
    field_name: &str,
    value: u32,
    min: u32,
    max: u32,
    strict_mode: bool,
) -> Result<u32, BridgeError> {
    if strict_mode && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange {
            field: field_name.to_owned(),
            value: f64::from(value),
            min: f64::from(min),
            max: f64::from(max),
        }
        .into());
    }
    Ok(value.clamp(min, max))
}

fn normalize_f32_range(
    field_name: &str,
    value: f32,
    min: f32,
    max: f32,
    strict_mode: bool,
) -> Result<f32, BridgeError> {
    if !value.is_finite() {
        return Err(InvalidRequest::NonFiniteNumber {
            field: field_name.to_owned(),
            value: f64::from(value),
        }
        .into());
    }
    if strict_mode && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange {
            field: field_name.to_owned(),
            value: f64::from(value),
            min: f64::from(min),
            max: f64::from(max),
        }
        .into());
    }
    Ok(value.clamp(min, max))
}

fn normalize_image_dimension(
    field_name: &str,
    value: u32,
    strict_mode: bool,
) -> Result<u32, BridgeError> {
    if strict_mode
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
    Ok(snapped.clamp(IMAGE_DIMENSION_MIN, IMAGE_DIMENSION_MAX))
}

/// Shrinks a canvas that exceeds the API pixel budget, keeping the 64 pixel grid.
fn normalize_canvas_area(size: ImageSize, strict_mode: bool) -> Result<ImageSize, BridgeError> {
    if size.pixels() <= MAX_IMAGE_PIXELS {
        return Ok(size);
    }
    if strict_mode {
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
    Ok(shrunk)
}
