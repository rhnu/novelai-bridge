use super::NormalizedGeneration;
use crate::{
    BridgeError, GenerateImageRequest, GenerationMode, InvalidRequest, ModelDescriptor,
    ResolvedPrompts, VibeTransferConfig,
    internal::{
        request_helpers::{
            build_v4_character_captions, convert_character_prompts, convert_character_references,
            default_seed, fit_mask_image, fit_source_image,
        },
        wire::{
            CharacterPrompt, ImageGenerationRequestBody, ImageParameters, V4Caption,
            V4ConditionInput,
        },
    },
};
use base64::{Engine, engine::general_purpose::STANDARD};

/// Image-prepared input for one sending attempt, with a fixed resolved seed.
///
/// This type cannot be deserialized or mutated into an unvalidated request. It does not send
/// network traffic or retry. Image preparation may allocate and decode image pixels.
#[derive(Debug)]
pub struct PreparedGeneration {
    normalized: NormalizedGeneration,
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

    pub(crate) fn into_parts(self) -> (NormalizedGeneration, ImageGenerationRequestBody, i64) {
        (self.normalized, self.body, self.resolved_seed)
    }
}

pub(super) fn prepare(normalized: NormalizedGeneration) -> Result<PreparedGeneration, BridgeError> {
    let resolved_seed = resolve_seed(normalized.request().core.seed);
    let body = build_generate_request(&normalized, resolved_seed)?;
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
    let image_parameters = build_image_parameters(normalized, action, resolved_seed)?;

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
    let img2img = build_img2img_wire_parts(params, action, resolved_seed)?;

    let references = params.references.as_ref();
    let vibe = references
        .and_then(|set| set.vibe_transfer.as_ref())
        .map(encode_vibe_references)
        .transpose()?;
    let character_references = references
        .map(|set| set.character_references.as_slice())
        .filter(|refs| !refs.is_empty())
        .map(convert_character_references)
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
        reference_image_multiple: vibe.as_ref().map(|value| value.images.clone()),
        reference_strength_multiple: vibe.as_ref().map(|value| value.strengths.clone()),
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
    if seed == 0 { default_seed() } else { seed }
}

fn encode_vibe_references(vibe: &VibeTransferConfig) -> Result<VibeWireParts, BridgeError> {
    let mut images = Vec::with_capacity(vibe.references.len());
    let mut strengths = Vec::with_capacity(vibe.references.len());

    for reference in &vibe.references {
        images.push(resolve_vibe_data_cache(&reference.vibe_data_cache)?);
        strengths.push(reference.strength);
    }

    Ok(VibeWireParts {
        images,
        strengths,
        strength: vibe.strength,
    })
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
        let char_prompts = build_v4_character_captions(
            characters,
            &resolved.characters,
            false,
            resolved_use_coords,
        );
        let char_negative_prompts = build_v4_character_captions(
            characters,
            &resolved.characters,
            true,
            resolved_use_coords,
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
) -> Result<Img2ImgWireParts, BridgeError> {
    let Some(img2img) = params.img2img.as_ref() else {
        return Ok(Img2ImgWireParts::default());
    };

    let image = fit_source_image("img2img.image", &img2img.image, params.core.size)?;

    if action == GenerationMode::Inpainting {
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

fn resolve_vibe_data_cache(value: &str) -> Result<String, BridgeError> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: "vibe_data_cache".to_owned(),
        }
        .into());
    }

    STANDARD
        .decode(compact.as_bytes())
        .map_err(|source| InvalidRequest::InvalidBase64 {
            field: "vibe_data_cache".to_owned(),
            source,
        })?;

    Ok(compact)
}
