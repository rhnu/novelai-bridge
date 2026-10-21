use super::{
    CapabilityParameters, CommonParameters, ConditioningParameters, ImageGenerationRequestBody,
    ImageParameters, NegativeCondition, OperationParameters, PositiveCondition, ProtocolExtensions,
    V4Caption, WireAction,
    prompt::{build_character_captions, build_character_prompts},
    references::prepare_references,
};
use crate::{
    BridgeError, GenerationField, GenerationInput, GenerationMode, ImageInputField,
    ImagePreparation, ImagePreprocessor, ModelDescriptor, ResolvedPrompts,
    generation::NormalizedGeneration,
};

pub(in crate::generation) fn build_generate_request(
    normalized: &NormalizedGeneration,
    resolved_seed: i64,
    preprocessor: Option<&dyn ImagePreprocessor>,
) -> Result<ImageGenerationRequestBody, BridgeError> {
    let params = normalized.request();
    let descriptor = params.descriptor();
    let common = CommonParameters {
        params_version: descriptor.wire().params_version(),
        width: params.core.size.width,
        height: params.core.size.height,
        steps: params.sampling.steps,
        scale: params.sampling.scale,
        sampler: params.sampling.sampler,
        seed: resolved_seed,
        n_samples: params.core.n_samples,
        uc_preset: params.core.uc_preset.as_api_value(),
        quality_toggle: params.core.quality.is_enabled(),
        cfg_rescale: params.sampling.cfg_rescale,
        noise_schedule: params.sampling.noise_schedule,
        legacy: false,
        legacy_v3_extend: false,
        deliberate_euler_ancestral_bug: false,
        prefer_brownian: true,
        auto_smea: false,
        image_format: params.output.image_format,
    };
    let capabilities = CapabilityParameters {
        skip_cfg_above_sigma: params
            .guidance
            .variety_boost
            .then(|| {
                descriptor
                    .sampling()
                    .variety_sigma(params.core.size.width, params.core.size.height)
            })
            .flatten(),
        sm: descriptor.sampling().smea.then_some(params.guidance.smea),
        sm_dyn: descriptor
            .sampling()
            .smea
            .then_some(params.guidance.smea_dyn),
        dynamic_thresholding: descriptor
            .sampling()
            .dynamic_thresholding
            .then_some(params.guidance.decrisp),
        tag_hint_transparent_background: descriptor
            .prompt()
            .transparent_background
            .then_some(params.core.transparent_background),
    };
    Ok(ImageGenerationRequestBody {
        action: match params.mode() {
            GenerationMode::TextToImage => WireAction::Generate,
            GenerationMode::ImageToImage => WireAction::ImageToImage,
            GenerationMode::Inpainting => WireAction::Inpainting,
        },
        input: params.core.prompt.clone(),
        model: normalized.model_id(),
        use_new_shared_trial: true,
        parameters: ImageParameters {
            common,
            conditioning: conditioning(normalized.resolved_prompts(), descriptor),
            operation: prepare_operation(normalized, resolved_seed, preprocessor)?,
            capabilities,
            references: prepare_references(params.references.as_ref(), preprocessor)?,
            extensions: if descriptor.wire().uses_v5_extensions() {
                ProtocolExtensions::V5 { legacy_uc: false }
            } else {
                ProtocolExtensions::Standard {}
            },
        },
    })
}

fn conditioning(
    resolved: &ResolvedPrompts,
    descriptor: &ModelDescriptor,
) -> ConditioningParameters {
    if descriptor.wire().uses_structured_prompt() {
        ConditioningParameters::Structured {
            negative_prompt: resolved.negative_prompt.clone(),
            v4_prompt: PositiveCondition {
                caption: V4Caption {
                    base_caption: resolved.prompt.clone(),
                    char_captions: build_character_captions(&resolved.characters, false),
                },
                use_coords: resolved.use_coords,
                use_order: true,
            },
            v4_negative_prompt: NegativeCondition {
                caption: V4Caption {
                    base_caption: resolved.negative_prompt.clone(),
                    char_captions: build_character_captions(&resolved.characters, true),
                },
            },
            character_prompts: build_character_prompts(&resolved.characters),
            use_coords: resolved.use_coords,
        }
    } else {
        ConditioningParameters::Legacy {
            prompt: resolved.prompt.clone(),
            negative_prompt: resolved.negative_prompt.clone(),
            uc: resolved.negative_prompt.clone(),
        }
    }
}

fn prepare_operation(
    normalized: &NormalizedGeneration,
    resolved_seed: i64,
    preprocessor: Option<&dyn ImagePreprocessor>,
) -> Result<OperationParameters, BridgeError> {
    let request = normalized.request();
    let prepare_image = |field, image, operation| {
        crate::image::prepare_image(
            preprocessor,
            GenerationField::ImageInput(field).into(),
            image,
            operation,
        )
        .map(|prepared| prepared.to_base64())
    };
    match &request.operation {
        GenerationInput::TextToImage => Ok(OperationParameters::TextToImage {
            add_original_image: true,
            inpaint_strength: 1.0,
        }),
        GenerationInput::ImageToImage(input) => Ok(OperationParameters::ImageToImage {
            image: prepare_image(
                ImageInputField::Image,
                &input.image,
                ImagePreparation::Source(request.core.size),
            )?,
            strength: input.strength,
            noise: input.noise,
            extra_noise_seed: input.extra_noise_seed.unwrap_or(resolved_seed),
            color_correct: input.color_correct,
            add_original_image: input.add_original_image,
            inpaint_strength: 1.0,
        }),
        GenerationInput::Inpainting(input) => Ok(OperationParameters::Inpainting {
            image: prepare_image(
                ImageInputField::Image,
                &input.image,
                ImagePreparation::Source(request.core.size),
            )?,
            mask: prepare_image(
                ImageInputField::Mask,
                &input.mask,
                ImagePreparation::Mask(request.core.size),
            )?,
            noise: input.noise,
            request_type: "NativeInfillingRequest",
            add_original_image: input.add_original_image,
            strength: input.strength,
        }),
    }
}
