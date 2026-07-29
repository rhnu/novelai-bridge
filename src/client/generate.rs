use crate::{
    BridgeError, Character, CharacterReference, ControlNetConfig, GenerateImageRequest,
    GenerateImageResult, GenerateImageStream, GenerateImageStreamRequest, Img2ImgRequest,
    InvalidRequest, Transport,
    internal::{
        request_helpers::{
            build_v4_character_captions, convert_character_prompts, convert_character_references,
            default_seed, extract_images, is_center_position, normalize_image_payload,
        },
        wire::{
            CharacterPrompt, ImageGenerationRequestBody, ImageParameters, ImageStreamParameters,
            StreamImageGenerationRequestBody, V4Caption, V4ConditionInput,
        },
    },
};

use super::Client;

struct ControlNetReferenceWireParts {
    images: Vec<String>,
    strengths: Vec<f32>,
    information_extracted: Vec<f32>,
}

struct PromptWireParts {
    prompt_field: Option<String>,
    negative_prompt: String,
    v4_prompt: Option<V4ConditionInput>,
    v4_negative_prompt: Option<V4ConditionInput>,
    character_prompts: Option<Vec<CharacterPrompt>>,
}

struct I2iWireParts {
    image: Option<String>,
    mask: Option<String>,
    strength: Option<f32>,
    noise: Option<f32>,
}

impl<T: Transport> Client<T> {
    /// Generates images from a request and returns the decoded image payloads.
    ///
    /// The client validates request ranges and payload shape locally, but it intentionally
    /// does not parse or reject prompt syntax before sending the request.
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
        let resolved_seed = Self::resolve_seed(params.seed);
        let request = Self::build_generate_request(&params, resolved_seed)?;
        let endpoint = self.image_endpoint("/ai/generate-image");
        let payload = self
            .transport
            .post_bytes(endpoint.as_str(), &request)
            .await?;

        Ok(GenerateImageResult {
            resolved_seed,
            images: extract_images(payload),
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
        let resolved_seed = Self::resolve_seed(params.base.seed);
        let request = Self::build_stream_request(&params, resolved_seed)?;
        let endpoint = self.image_endpoint("/ai/generate-image-stream");
        let stream = self.transport.post_sse(endpoint.as_str(), &request).await?;
        Ok(GenerateImageStream::new(resolved_seed, stream))
    }

    fn normalize_generate_request(
        mut params: GenerateImageRequest,
    ) -> Result<GenerateImageRequest, BridgeError> {
        validate_prompt(&params)?;
        let strict_mode = params.strict_mode;
        normalize_base_fields(&mut params, strict_mode)?;
        validate_model_features(&params)?;
        validate_character_inputs(params.characters.as_deref(), strict_mode)?;
        normalize_character_positions(params.characters.as_mut())?;
        normalize_character_references(params.character_references.as_mut(), strict_mode)?;
        normalize_i2i(params.i2i.as_mut(), strict_mode)?;
        normalize_controlnet(params.controlnet.as_mut(), strict_mode)?;

        Ok(params)
    }

    fn build_generate_request(
        params: &GenerateImageRequest,
        resolved_seed: i64,
    ) -> Result<ImageGenerationRequestBody, BridgeError> {
        let image_parameters = Self::build_image_parameters(params, resolved_seed)?;

        Ok(ImageGenerationRequestBody {
            action: "generate".to_owned(),
            input: params.prompt.clone(),
            model: params.model.as_str().to_owned(),
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

    fn build_image_parameters(
        params: &GenerateImageRequest,
        resolved_seed: i64,
    ) -> Result<ImageParameters, BridgeError> {
        let resolved_use_coords = Self::resolve_use_coords(params);
        let prompt = build_prompt_wire_parts(params, resolved_use_coords);
        let controlnet_encoded = Self::encode_controlnet(params.controlnet.as_ref())?;
        let character_references =
            convert_character_references(params.character_references.as_deref())?;
        let i2i = build_i2i_wire_parts(params)?;

        Ok(ImageParameters {
            width: Some(params.size.width),
            height: Some(params.size.height),
            steps: Some(params.steps),
            scale: Some(params.scale),
            sampler: Some(params.sampler.as_str().to_owned()),
            seed: Some(resolved_seed),
            n_samples: Some(params.n_samples),
            prompt: prompt.prompt_field,
            negative_prompt: Some(prompt.negative_prompt),
            uc_preset: Some(params.uc_preset.as_api_value()),
            quality_toggle: Some(params.quality),
            v4_prompt: prompt.v4_prompt,
            v4_negative_prompt: prompt.v4_negative_prompt,
            cfg_rescale: Some(params.cfg_rescale),
            noise_schedule: Some(params.noise_schedule.as_str().to_owned()),
            skip_cfg_above_sigma: if params.variety_boost { Some(58) } else { None },
            image: i2i.image,
            strength: i2i.strength,
            noise: i2i.noise,
            mask: i2i.mask,
            reference_image_multiple: controlnet_encoded
                .as_ref()
                .map(|value| value.images.clone()),
            reference_strength_multiple: controlnet_encoded
                .as_ref()
                .map(|value| value.strengths.clone()),
            reference_information_extracted_multiple: controlnet_encoded
                .as_ref()
                .map(|value| value.information_extracted.clone()),
            normalize_reference_strength_multiple: Some(true),
            controlnet_strength: params.controlnet.as_ref().map(|cn| cn.strength),
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
            params_version: Some(3),
            legacy: Some(false),
            legacy_uc: Some(false),
            legacy_v3_extend: Some(false),
            deliberate_euler_ancestral_bug: Some(false),
            prefer_brownian: Some(true),
            auto_smea: Some(false),
            sm: Some(false),
            sm_dyn: Some(false),
            dynamic_thresholding: Some(false),
            add_original_image: Some(false),
            inpaint_img2_img_strength: Some(1.0),
            use_coords: Some(resolved_use_coords),
            image_format: params.image_format.map(|fmt| fmt.as_str().to_owned()),
        })
    }

    fn resolve_seed(seed: i64) -> i64 {
        if seed == 0 { default_seed() } else { seed }
    }

    fn resolve_use_coords(params: &GenerateImageRequest) -> bool {
        let Some(characters) = params.characters.as_ref() else {
            return false;
        };
        let has_enabled = characters.iter().any(|character| character.enabled);
        if !has_enabled {
            return false;
        }
        if let Some(explicit) = params.use_coords {
            return explicit;
        }
        characters.iter().any(|character| {
            character.enabled
                && (!is_center_position(character.position.x)
                    || !is_center_position(character.position.y))
        })
    }

    fn encode_controlnet(
        controlnet: Option<&ControlNetConfig>,
    ) -> Result<Option<ControlNetReferenceWireParts>, BridgeError> {
        let Some(controlnet) = controlnet else {
            return Ok(None);
        };

        let mut encoded = Vec::with_capacity(controlnet.images.len());
        let mut strengths = Vec::with_capacity(controlnet.images.len());
        let mut info_extracted = Vec::with_capacity(controlnet.images.len());

        for image in &controlnet.images {
            let vibe = Self::resolve_vibe_data_cache(&image.vibe_data_cache)?;
            encoded.push(vibe);
            strengths.push(image.strength);
            info_extracted.push(image.info_extracted);
        }

        Ok(Some(ControlNetReferenceWireParts {
            images: encoded,
            strengths,
            information_extracted: info_extracted,
        }))
    }
}

fn validate_prompt(params: &GenerateImageRequest) -> Result<(), BridgeError> {
    if params.prompt.trim().is_empty() {
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
    params.steps = normalize_u32_range("steps", params.steps, 1, 50, strict_mode)?;
    params.scale = normalize_f32_range("scale", params.scale, 0.0, 10.0, strict_mode)?;
    params.n_samples = normalize_u32_range("n_samples", params.n_samples, 1, 4, strict_mode)?;
    params.cfg_rescale =
        normalize_f32_range("cfg_rescale", params.cfg_rescale, 0.0, 1.0, strict_mode)?;
    params.size.width = normalize_image_dimension("size.width", params.size.width, strict_mode)?;
    params.size.height = normalize_image_dimension("size.height", params.size.height, strict_mode)?;
    Ok(())
}

fn validate_model_features(params: &GenerateImageRequest) -> Result<(), BridgeError> {
    if params
        .character_references
        .as_ref()
        .is_some_and(|refs| !refs.is_empty())
        && !params.model.is_v45()
    {
        return Err(InvalidRequest::UnsupportedModelFeature {
            feature: "character_references".to_owned(),
            required_model: "V4.5 models".to_owned(),
        }
        .into());
    }

    if params
        .characters
        .as_ref()
        .is_some_and(|chars| !chars.is_empty())
        && !params.model.is_v4()
    {
        return Err(InvalidRequest::UnsupportedModelFeature {
            feature: "characters".to_owned(),
            required_model: "V4/V4.5 models".to_owned(),
        }
        .into());
    }

    if params
        .controlnet
        .as_ref()
        .is_some_and(|controlnet| !controlnet.images.is_empty())
        && params
            .character_references
            .as_ref()
            .is_some_and(|refs| !refs.is_empty())
    {
        return Err(InvalidRequest::UnsupportedFieldCombination {
            left: "controlnet".to_owned(),
            right: "character_references".to_owned(),
        }
        .into());
    }

    Ok(())
}

fn validate_character_inputs(
    characters: Option<&[Character]>,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(characters) = characters else {
        return Ok(());
    };

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

fn normalize_character_positions(
    characters: Option<&mut Vec<Character>>,
) -> Result<(), BridgeError> {
    let Some(characters) = characters else {
        return Ok(());
    };

    for ch in characters {
        ch.position.x =
            normalize_f32_range("character.position.x", ch.position.x, 0.0, 1.0, false)?;
        ch.position.y =
            normalize_f32_range("character.position.y", ch.position.y, 0.0, 1.0, false)?;
    }

    Ok(())
}

fn normalize_character_references(
    refs: Option<&mut Vec<CharacterReference>>,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(refs) = refs else {
        return Ok(());
    };

    for (idx, item) in refs.iter_mut().enumerate() {
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

fn normalize_i2i(i2i: Option<&mut Img2ImgRequest>, strict_mode: bool) -> Result<(), BridgeError> {
    let Some(i2i) = i2i else {
        return Ok(());
    };

    i2i.strength = normalize_f32_range("i2i.strength", i2i.strength, 0.01, 0.99, strict_mode)?;
    i2i.noise = normalize_f32_range("i2i.noise", i2i.noise, 0.0, 0.99, strict_mode)?;
    Ok(())
}

fn normalize_controlnet(
    controlnet: Option<&mut ControlNetConfig>,
    strict_mode: bool,
) -> Result<(), BridgeError> {
    let Some(controlnet) = controlnet else {
        return Ok(());
    };

    controlnet.strength = normalize_f32_range(
        "controlnet.strength",
        controlnet.strength,
        0.0,
        1.0,
        strict_mode,
    )?;
    if controlnet.images.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: "controlnet.images".to_owned(),
        }
        .into());
    }

    for (idx, image) in controlnet.images.iter_mut().enumerate() {
        if image.vibe_data_cache.trim().is_empty() {
            return Err(InvalidRequest::EmptyField {
                field: format!("controlnet.images[{idx}].vibe_data_cache"),
            }
            .into());
        }
        image.info_extracted = normalize_f32_range(
            &format!("controlnet.images[{idx}].info_extracted"),
            image.info_extracted,
            0.01,
            1.0,
            strict_mode,
        )?;
        image.strength = normalize_f32_range(
            &format!("controlnet.images[{idx}].strength"),
            image.strength,
            0.0,
            1.0,
            strict_mode,
        )?;
    }

    Ok(())
}

fn build_prompt_wire_parts(
    params: &GenerateImageRequest,
    resolved_use_coords: bool,
) -> PromptWireParts {
    let processed_prompt = params.processed_prompt();
    let negative_prompt = params.processed_negative_prompt();
    let character_prompts = build_character_prompts_field(params, resolved_use_coords);

    if params.model.is_v4() {
        let char_prompts =
            build_v4_character_captions(params.characters.as_deref(), false, resolved_use_coords);
        let char_negative_prompts =
            build_v4_character_captions(params.characters.as_deref(), true, resolved_use_coords);

        return PromptWireParts {
            prompt_field: None,
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
                legacy_uc: Some(false),
                use_coords: None,
                use_order: None,
            }),
            negative_prompt,
            character_prompts,
        };
    }

    PromptWireParts {
        prompt_field: Some(processed_prompt),
        negative_prompt,
        v4_prompt: None,
        v4_negative_prompt: None,
        character_prompts,
    }
}

fn build_character_prompts_field(
    params: &GenerateImageRequest,
    resolved_use_coords: bool,
) -> Option<Vec<CharacterPrompt>> {
    if params.model.is_v4() {
        return Some(
            params
                .characters
                .as_ref()
                .map(|characters| convert_character_prompts(characters, resolved_use_coords))
                .unwrap_or_default(),
        );
    }

    params
        .characters
        .as_ref()
        .map(|characters| convert_character_prompts(characters, resolved_use_coords))
}

fn build_i2i_wire_parts(params: &GenerateImageRequest) -> Result<I2iWireParts, BridgeError> {
    let Some(i2i) = params.i2i.as_ref() else {
        return Ok(I2iWireParts {
            image: None,
            mask: None,
            strength: None,
            noise: None,
        });
    };

    let image = normalize_image_payload("i2i.image", &i2i.image)?;
    let mask = i2i
        .mask
        .as_ref()
        .map(|mask| normalize_image_payload("i2i.mask", mask))
        .transpose()?;

    Ok(I2iWireParts {
        image: Some(image),
        mask,
        strength: Some(i2i.strength),
        noise: Some(i2i.noise),
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
    if strict_mode && (!(64..=1600).contains(&value) || !value.is_multiple_of(64)) {
        return Err(InvalidRequest::InvalidImageDimension {
            field: field_name.to_owned(),
            value,
            min: 64,
            max: 1600,
            multiple_of: 64,
        }
        .into());
    }

    let clamped = value.clamp(64, 1600);
    let snapped = ((clamped + 32) / 64) * 64;
    Ok(snapped.clamp(64, 1600))
}
