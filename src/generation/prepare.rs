use super::NormalizedGeneration;
use crate::{
    BridgeError, DefaultImagePreprocessor, GenerateImageRequest, GenerationMode, ImagePreparation,
    ImagePreprocessor, InvalidRequest, ModelDescriptor, ResolvedPrompts, VibeTransferConfig,
    generation::{
        prompt::{
            build_v4_character_captions, convert_character_prompts, convert_character_references,
        },
        wire::{
            CharacterPrompt, ImageGenerationRequestBody, ImageParameters, V4Caption,
            V4ConditionInput,
        },
    },
};

/// Image-prepared input for one sending attempt, with a fixed resolved seed.
///
/// This type cannot be deserialized or mutated into an unvalidated request. It does not send
/// network traffic or retry. Image preparation may allocate and decode image pixels.
#[derive(Debug)]
pub struct PreparedGeneration {
    normalized: NormalizedGeneration,
    #[cfg_attr(not(feature = "client"), allow(dead_code))]
    body: ImageGenerationRequestBody,
    resolved_seed: i64,
}

impl PreparedGeneration {
    /// Effective input and cached prompts used to construct this payload.
    #[must_use]
    pub const fn normalized(&self) -> &NormalizedGeneration {
        &self.normalized
    }

    /// Request-level seed selected once during preparation.
    #[must_use]
    pub const fn resolved_seed(&self) -> i64 {
        self.resolved_seed
    }

    #[cfg_attr(not(feature = "client"), allow(dead_code))]
    pub(crate) fn into_parts(self) -> (NormalizedGeneration, ImageGenerationRequestBody, i64) {
        (self.normalized, self.body, self.resolved_seed)
    }
}

pub(super) fn prepare(normalized: NormalizedGeneration) -> Result<PreparedGeneration, BridgeError> {
    prepare_with(normalized, &DefaultImagePreprocessor)
}

pub(super) fn prepare_with(
    normalized: NormalizedGeneration,
    preprocessor: &dyn ImagePreprocessor,
) -> Result<PreparedGeneration, BridgeError> {
    let resolved_seed = resolve_seed(normalized.request().core.seed);
    let body = build_generate_request(&normalized, resolved_seed, preprocessor)?;
    Ok(PreparedGeneration {
        normalized,
        body,
        resolved_seed,
    })
}

struct VibeWireParts {
    images: Vec<String>,
    strengths: Vec<f32>,
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

fn build_generate_request(
    normalized: &NormalizedGeneration,
    resolved_seed: i64,
    preprocessor: &dyn ImagePreprocessor,
) -> Result<ImageGenerationRequestBody, BridgeError> {
    let params = normalized.request();
    let action = params.mode();
    let model = normalized
        .request()
        .descriptor()
        .model_id_for(action)
        .ok_or_else(|| InvalidRequest::UnsupportedModelFeature {
            feature: "img2img.mask".to_owned(),
            required_model: "models with an inpainting variant".to_owned(),
        })?;
    let image_parameters = build_image_parameters(normalized, action, resolved_seed, preprocessor)?;

    Ok(ImageGenerationRequestBody {
        action: match action {
            GenerationMode::TextToImage => "generate",
            GenerationMode::ImageToImage => "img2img",
            GenerationMode::Inpainting => "infill",
        }
        .to_owned(),
        input: params.core.prompt.clone(),
        model: model.to_owned(),
        use_new_shared_trial: true,
        parameters: image_parameters,
    })
}

#[allow(clippy::too_many_lines)]
fn build_image_parameters(
    normalized: &NormalizedGeneration,
    action: GenerationMode,
    resolved_seed: i64,
    preprocessor: &dyn ImagePreprocessor,
) -> Result<ImageParameters, BridgeError> {
    let params = normalized.request();
    let descriptor = params.core.model.descriptor();
    let uses_v4 = descriptor.wire().uses_structured_prompt();
    let resolved_use_coords = params
        .characters
        .as_ref()
        .and_then(|set| set.use_coords)
        .unwrap_or(false);
    let prompt = build_prompt_wire_parts(
        params,
        normalized.resolved_prompts(),
        descriptor,
        resolved_use_coords,
    );
    let img2img = build_img2img_wire_parts(params, action, resolved_seed, preprocessor)?;

    let references = params.references.as_ref();
    let vibe = references
        .and_then(|set| set.vibe_transfer.as_ref())
        .map(encode_vibe_references);
    let character_references = references
        .map(|set| set.character_references.as_slice())
        .filter(|refs| !refs.is_empty())
        .map(|refs| convert_character_references(refs, preprocessor))
        .transpose()?;

    let variety_sigma = params
        .guidance
        .variety_boost
        .then(|| {
            descriptor
                .sampling()
                .variety_sigma(params.core.size.width, params.core.size.height)
        })
        .flatten();

    let (
        reference_image_multiple,
        reference_strength_multiple,
        normalize_reference_strength_multiple,
        controlnet_strength,
    ) = match vibe {
        Some(VibeWireParts {
            images,
            strengths,
            strength,
        }) => (Some(images), Some(strengths), Some(true), strength),
        None => (None, None, None, 1.0),
    };
    let (
        director_reference_images,
        director_reference_descriptions,
        director_reference_information_extracted,
        director_reference_secondary_strength_values,
        director_reference_strength_values,
    ) = match character_references {
        Some(parts) => (
            Some(parts.images),
            Some(parts.descriptions),
            Some(parts.information_extracted),
            Some(parts.secondary_strength_values),
            Some(parts.strength_values),
        ),
        None => (None, None, None, None, None),
    };

    Ok(ImageParameters {
        params_version: Some(descriptor.wire().params_version()),
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
        quality_toggle: Some(params.core.quality.is_enabled()),
        v4_prompt: prompt.v4_prompt,
        v4_negative_prompt: prompt.v4_negative_prompt,
        cfg_rescale: Some(params.sampling.cfg_rescale),
        noise_schedule: Some(params.sampling.noise_schedule.as_str().to_owned()),
        skip_cfg_above_sigma: variety_sigma,
        request_type: img2img.request_type,
        image: img2img.image,
        strength: img2img.strength,
        noise: img2img.noise,
        extra_noise_seed: img2img.extra_noise_seed,
        color_correct: img2img.color_correct,
        mask: img2img.mask,
        reference_image_multiple,
        reference_strength_multiple,
        normalize_reference_strength_multiple,
        controlnet_strength: Some(controlnet_strength),
        director_reference_images,
        director_reference_descriptions,
        director_reference_information_extracted,
        director_reference_secondary_strength_values,
        director_reference_strength_values,
        character_prompts: prompt.character_prompts,
        legacy: Some(false),
        legacy_uc: descriptor.wire().uses_v5_extensions().then_some(false),
        legacy_v3_extend: Some(false),
        deliberate_euler_ancestral_bug: Some(false),
        prefer_brownian: Some(true),
        auto_smea: Some(false),
        sm: descriptor.sampling().smea.then_some(params.guidance.smea),
        sm_dyn: descriptor
            .sampling()
            .smea
            .then_some(params.guidance.smea_dyn),
        dynamic_thresholding: descriptor
            .sampling()
            .dynamic_thresholding
            .then_some(params.guidance.decrisp),
        add_original_image: Some(
            params
                .img2img
                .as_ref()
                .is_none_or(|img2img| img2img.add_original_image),
        ),
        inpaint_img2_img_strength: Some(img2img.inpaint_strength.unwrap_or(1.0)),
        tag_hint_transparent_background: descriptor
            .prompt()
            .transparent_background
            .then_some(params.core.transparent_background),
        use_coords: uses_v4.then_some(resolved_use_coords),
        image_format: params
            .output
            .image_format
            .map(|format| format.as_str().to_owned()),
    })
}

fn resolve_seed(seed: i64) -> i64 {
    if seed == 0 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |duration| {
                let seed = (duration.as_millis() % 10_000_000_000) as i64;
                if seed == 0 { 1 } else { seed }
            })
    } else {
        seed
    }
}

fn encode_vibe_references(vibe: &VibeTransferConfig) -> VibeWireParts {
    let mut images = Vec::with_capacity(vibe.references.len());
    let mut strengths = Vec::with_capacity(vibe.references.len());

    for reference in &vibe.references {
        images.push(reference.vibe_data_cache.to_base64());
        strengths.push(reference.strength);
    }

    VibeWireParts {
        images,
        strengths,
        strength: vibe.strength,
    }
}

fn build_prompt_wire_parts(
    params: &GenerateImageRequest,
    resolved: &ResolvedPrompts,
    descriptor: &ModelDescriptor,
    resolved_use_coords: bool,
) -> PromptWireParts {
    let characters = params
        .characters
        .as_ref()
        .map(|set| set.characters.as_slice())
        .unwrap_or_default();

    if descriptor.wire().uses_structured_prompt() {
        let positioning = descriptor
            .prompt()
            .characters
            .expect("structured models support character prompts")
            .positioning;
        let char_prompts = build_v4_character_captions(
            characters,
            &resolved.characters,
            false,
            resolved_use_coords,
            positioning,
        );
        let char_negative_prompts = build_v4_character_captions(
            characters,
            &resolved.characters,
            true,
            resolved_use_coords,
            positioning,
        );

        return PromptWireParts {
            prompt_field: None,
            legacy_uc: None,
            v4_prompt: Some(V4ConditionInput {
                caption: Some(V4Caption {
                    base_caption: Some(resolved.prompt.clone()),
                    char_captions: Some(char_prompts),
                }),
                legacy_uc: None,
                use_coords: Some(resolved_use_coords),
                use_order: Some(true),
            }),
            v4_negative_prompt: Some(V4ConditionInput {
                caption: Some(V4Caption {
                    base_caption: Some(resolved.negative_prompt.clone()),
                    char_captions: Some(char_negative_prompts),
                }),
                legacy_uc: None,
                use_coords: None,
                use_order: None,
            }),
            negative_prompt: resolved.negative_prompt.clone(),
            character_prompts: Some(convert_character_prompts(
                characters,
                &resolved.characters,
                resolved_use_coords,
                positioning,
            )),
        };
    }

    PromptWireParts {
        prompt_field: Some(resolved.prompt.clone()),
        legacy_uc: Some(resolved.negative_prompt.clone()),
        negative_prompt: resolved.negative_prompt.clone(),
        v4_prompt: None,
        v4_negative_prompt: None,
        character_prompts: None,
    }
}

fn build_img2img_wire_parts(
    params: &GenerateImageRequest,
    action: GenerationMode,
    resolved_seed: i64,
    preprocessor: &dyn ImagePreprocessor,
) -> Result<Img2ImgWireParts, BridgeError> {
    let Some(img2img) = params.img2img.as_ref() else {
        return Ok(Img2ImgWireParts::default());
    };

    let image = preprocessor
        .prepare(
            "img2img.image",
            &img2img.image,
            ImagePreparation::Source(params.core.size),
        )?
        .to_base64();

    if action == GenerationMode::Inpainting {
        let mask = img2img
            .mask
            .as_ref()
            .map(|mask| {
                preprocessor
                    .prepare(
                        "img2img.mask",
                        mask,
                        ImagePreparation::Mask(params.core.size),
                    )
                    .map(|image| image.to_base64())
            })
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
