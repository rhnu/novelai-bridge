use crate::{
    BridgeError, ControlNetConfig, GenerateImageRequest, GenerateImageStreamRequest,
    GeneratedImage, ImageStreamResult, NoiseSchedule, Transport,
    internal::{
        request_helpers::{
            build_v4_character_captions, convert_character_prompts, convert_character_references,
            default_seed, extract_images, is_center_position, normalize_image_payload,
        },
        transport_helpers::map_sse_stream,
        wire::{
            ImageGenerationRequestBody, ImageParameters, ImageStreamParameters,
            StreamImageGenerationRequestBody, V4Caption, V4ConditionInput,
        },
    },
};

use super::Client;

impl<T: Transport> Client<T> {
    /// Generates images from a request and returns the decoded image payloads.
    ///
    /// The client validates request ranges and payload shape locally, but it intentionally
    /// does not parse or reject prompt syntax before sending the request.
    pub async fn generate(
        &self,
        params: GenerateImageRequest,
    ) -> Result<Vec<GeneratedImage>, BridgeError> {
        let params = self.normalize_generate_request(params)?;
        let resolved_seed = Self::resolve_seed(params.seed);
        let request = self.build_generate_request(&params, resolved_seed).await?;
        let payload = self
            .transport
            .post_bytes("/ai/generate-image", &request)
            .await?;

        Ok(extract_images(payload, resolved_seed))
    }

    /// Starts a streaming image generation request and returns parsed SSE chunks.
    pub async fn generate_stream(
        &self,
        params: GenerateImageStreamRequest,
    ) -> Result<ImageStreamResult, BridgeError> {
        let params = GenerateImageStreamRequest {
            base: self.normalize_generate_request(params.base)?,
            stream: params.stream,
        };
        let resolved_seed = Self::resolve_seed(params.base.seed);
        let request = self.build_stream_request(&params, resolved_seed).await?;
        let stream = self
            .transport
            .post_sse("/ai/generate-image-stream", &request)
            .await?;
        Ok(map_sse_stream(stream))
    }

    fn normalize_generate_request(
        &self,
        mut params: GenerateImageRequest,
    ) -> Result<GenerateImageRequest, BridgeError> {
        if params.prompt.trim().is_empty() {
            return Err(BridgeError::invalid_request("prompt cannot be empty."));
        }

        let strict_mode = params.strict_mode;
        params.steps = normalize_u32_range(
            "steps",
            params.steps,
            1,
            50,
            strict_mode,
            "steps must be between 1 and 50.",
        )?;
        params.scale = normalize_f32_range(
            "scale",
            params.scale,
            0.0,
            10.0,
            strict_mode,
            "scale must be between 0 and 10.",
        )?;
        params.n_samples = normalize_u32_range(
            "n_samples",
            params.n_samples,
            1,
            4,
            strict_mode,
            "n_samples must be between 1 and 4.",
        )?;
        params.cfg_rescale = normalize_f32_range(
            "cfg_rescale",
            params.cfg_rescale,
            0.0,
            1.0,
            strict_mode,
            "cfg_rescale must be between 0 and 1.",
        )?;
        params.size.width = normalize_image_dimension(params.size.width, strict_mode)?;
        params.size.height = normalize_image_dimension(params.size.height, strict_mode)?;

        if params
            .character_references
            .as_ref()
            .is_some_and(|refs| !refs.is_empty())
            && !params.model.is_v45()
        {
            return Err(BridgeError::invalid_request(
                "character_references are only supported on V4.5 models.",
            ));
        }

        if params
            .characters
            .as_ref()
            .is_some_and(|chars| !chars.is_empty())
            && !params.model.is_v4()
        {
            return Err(BridgeError::invalid_request(
                "characters are only supported on V4/V4.5 models.",
            ));
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
            return Err(BridgeError::invalid_request(
                "controlnet and character_references cannot be used together.",
            ));
        }

        if let Some(chars) = &params.characters {
            for (idx, ch) in chars.iter().enumerate() {
                if ch.prompt.trim().is_empty() {
                    return Err(BridgeError::invalid_request(format!(
                        "characters[{idx}].prompt cannot be empty."
                    )));
                }
                if strict_mode
                    && (!(0.0..=1.0).contains(&ch.position.x)
                        || !(0.0..=1.0).contains(&ch.position.y))
                {
                    return Err(BridgeError::invalid_request(format!(
                        "characters[{idx}].position must be in [0, 1]."
                    )));
                }
            }
        }

        if let Some(chars) = &mut params.characters {
            for ch in chars {
                ch.position.x = normalize_f32_range(
                    "character.position.x",
                    ch.position.x,
                    0.0,
                    1.0,
                    false,
                    "characters position must be in [0, 1].",
                )?;
                ch.position.y = normalize_f32_range(
                    "character.position.y",
                    ch.position.y,
                    0.0,
                    1.0,
                    false,
                    "characters position must be in [0, 1].",
                )?;
            }
        }

        if let Some(refs) = &mut params.character_references {
            for (idx, item) in refs.iter_mut().enumerate() {
                item.fidelity = normalize_f32_range(
                    &format!("character_references[{idx}].fidelity"),
                    item.fidelity,
                    0.0,
                    1.0,
                    strict_mode,
                    "character_references fidelity must be between 0 and 1.",
                )?;
                item.strength = normalize_f32_range(
                    &format!("character_references[{idx}].strength"),
                    item.strength,
                    0.0,
                    1.0,
                    strict_mode,
                    "character_references strength must be between 0 and 1.",
                )?;
            }
        }

        if let Some(i2i) = &mut params.i2i {
            i2i.strength = normalize_f32_range(
                "i2i.strength",
                i2i.strength,
                0.01,
                0.99,
                strict_mode,
                "i2i.strength must be between 0.01 and 0.99.",
            )?;
            i2i.noise = normalize_f32_range(
                "i2i.noise",
                i2i.noise,
                0.0,
                0.99,
                strict_mode,
                "i2i.noise must be between 0.0 and 0.99.",
            )?;
        }

        if let Some(controlnet) = &mut params.controlnet {
            controlnet.strength = normalize_f32_range(
                "controlnet.strength",
                controlnet.strength,
                0.0,
                1.0,
                strict_mode,
                "controlnet.strength must be between 0 and 1.",
            )?;
            if controlnet.images.is_empty() {
                return Err(BridgeError::invalid_request(
                    "controlnet.images cannot be empty.",
                ));
            }

            for (idx, image) in controlnet.images.iter_mut().enumerate() {
                let has_image = !image.image.trim().is_empty();
                let has_cache = image
                    .vibe_data_cache
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty());
                if !has_image && !has_cache {
                    return Err(BridgeError::invalid_request(format!(
                        "controlnet.images[{idx}] must provide image or vibe_data_cache."
                    )));
                }
                image.info_extracted = normalize_f32_range(
                    &format!("controlnet.images[{idx}].info_extracted"),
                    image.info_extracted,
                    0.01,
                    1.0,
                    strict_mode,
                    "controlnet.images info_extracted must be between 0.01 and 1.0.",
                )?;
                image.strength = normalize_f32_range(
                    &format!("controlnet.images[{idx}].strength"),
                    image.strength,
                    0.0,
                    1.0,
                    strict_mode,
                    "controlnet.images strength must be between 0 and 1.",
                )?;
            }
        }

        Ok(params)
    }

    async fn build_generate_request(
        &self,
        params: &GenerateImageRequest,
        resolved_seed: i64,
    ) -> Result<ImageGenerationRequestBody, BridgeError> {
        let image_parameters = self.build_image_parameters(params, resolved_seed).await?;

        Ok(ImageGenerationRequestBody {
            action: "generate".to_owned(),
            input: params.prompt.clone(),
            model: params.model.as_str().to_owned(),
            use_new_shared_trial: true,
            parameters: image_parameters,
        })
    }

    async fn build_stream_request(
        &self,
        params: &GenerateImageStreamRequest,
        resolved_seed: i64,
    ) -> Result<StreamImageGenerationRequestBody, BridgeError> {
        let base = self
            .build_generate_request(&params.base, resolved_seed)
            .await?;
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

    async fn build_image_parameters(
        &self,
        params: &GenerateImageRequest,
        resolved_seed: i64,
    ) -> Result<ImageParameters, BridgeError> {
        let processed_prompt = params.processed_prompt();
        let processed_negative_prompt = params.processed_negative_prompt();
        let resolved_use_coords = Self::resolve_use_coords(params);

        let (v4_prompt, v4_negative_prompt, prompt_field) = if params.model.is_v4() {
            let char_prompts = build_v4_character_captions(
                params.characters.as_deref(),
                false,
                resolved_use_coords,
            );
            let char_negative_prompts = build_v4_character_captions(
                params.characters.as_deref(),
                true,
                resolved_use_coords,
            );

            (
                Some(V4ConditionInput {
                    caption: Some(V4Caption {
                        base_caption: Some(processed_prompt.clone()),
                        char_captions: Some(char_prompts),
                    }),
                    legacy_uc: None,
                    use_coords: Some(resolved_use_coords),
                    use_order: Some(true),
                }),
                Some(V4ConditionInput {
                    caption: Some(V4Caption {
                        base_caption: Some(processed_negative_prompt.clone()),
                        char_captions: Some(char_negative_prompts),
                    }),
                    legacy_uc: Some(false),
                    use_coords: None,
                    use_order: None,
                }),
                None,
            )
        } else {
            (None, None, Some(processed_prompt.clone()))
        };

        let controlnet_encoded = self.encode_controlnet(params.controlnet.as_ref()).await?;
        let character_references = convert_character_references(params)?;
        let normalized_i2i_image = params
            .i2i
            .as_ref()
            .map(|i2i| normalize_image_payload("i2i.image", &i2i.image))
            .transpose()?;
        let normalized_i2i_mask = params
            .i2i
            .as_ref()
            .and_then(|i2i| i2i.mask.as_ref())
            .map(|mask| normalize_image_payload("i2i.mask", mask))
            .transpose()?;

        Ok(ImageParameters {
            width: Some(params.size.width),
            height: Some(params.size.height),
            steps: Some(params.steps),
            scale: Some(params.scale),
            sampler: Some(params.sampler.as_str().to_owned()),
            seed: Some(resolved_seed),
            n_samples: Some(params.n_samples),
            prompt: prompt_field,
            negative_prompt: Some(processed_negative_prompt),
            uc_preset: Some(params.uc_preset.as_api_value()),
            quality_toggle: Some(params.quality),
            v4_prompt,
            v4_negative_prompt,
            cfg_rescale: Some(params.cfg_rescale),
            noise_schedule: Some(match params.noise_schedule {
                NoiseSchedule::Karras => "karras".to_owned(),
                NoiseSchedule::Exponential => "exponential".to_owned(),
                NoiseSchedule::Polyexponential => "polyexponential".to_owned(),
            }),
            skip_cfg_above_sigma: if params.variety_boost { Some(58) } else { None },
            image: normalized_i2i_image,
            strength: params.i2i.as_ref().map(|i2i| i2i.strength),
            noise: params.i2i.as_ref().map(|i2i| i2i.noise),
            mask: normalized_i2i_mask,
            reference_image_multiple: controlnet_encoded.as_ref().map(|value| value.0.clone()),
            reference_strength_multiple: controlnet_encoded.as_ref().map(|value| value.1.clone()),
            reference_information_extracted_multiple: controlnet_encoded
                .as_ref()
                .map(|value| value.2.clone()),
            normalize_reference_strength_multiple: Some(true),
            controlnet_strength: params.controlnet.as_ref().map(|cn| cn.strength),
            director_reference_images: character_references.as_ref().map(|value| value.0.clone()),
            director_reference_descriptions: character_references
                .as_ref()
                .map(|value| value.1.clone()),
            director_reference_information_extracted: character_references
                .as_ref()
                .map(|value| value.2.clone()),
            director_reference_secondary_strength_values: character_references
                .as_ref()
                .map(|value| value.3.clone()),
            director_reference_strength_values: character_references
                .as_ref()
                .map(|value| value.4.clone()),
            character_prompts: if params.model.is_v4() {
                Some(
                    params
                        .characters
                        .as_ref()
                        .map(|characters| {
                            convert_character_prompts(characters, resolved_use_coords)
                        })
                        .unwrap_or_default(),
                )
            } else {
                params
                    .characters
                    .as_ref()
                    .map(|characters| convert_character_prompts(characters, resolved_use_coords))
            },
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

    async fn encode_controlnet(
        &self,
        controlnet: Option<&ControlNetConfig>,
    ) -> Result<Option<(Vec<String>, Vec<f32>, Vec<f32>)>, BridgeError> {
        let Some(controlnet) = controlnet else {
            return Ok(None);
        };

        let mut encoded = Vec::with_capacity(controlnet.images.len());
        let mut strengths = Vec::with_capacity(controlnet.images.len());
        let mut info_extracted = Vec::with_capacity(controlnet.images.len());

        for (idx, image) in controlnet.images.iter().enumerate() {
            if let Some(vibe_data_cache) = image
                .vibe_data_cache
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            {
                let vibe = self.resolve_vibe_data_cache(vibe_data_cache)?;
                encoded.push(vibe);
                strengths.push(image.strength);
                info_extracted.push(image.info_extracted);
                continue;
            }

            let normalized_image =
                normalize_image_payload(&format!("controlnet.images[{idx}].image"), &image.image)?;
            let cache_key = format!(
                "{}:{}:{}",
                image.model.as_str(),
                image.info_extracted,
                normalized_image.as_str()
            );

            let cached = self
                .vibe_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&cache_key).cloned());

            let vibe = if let Some(cached) = cached {
                cached
            } else {
                let encoded_vibe = self
                    .encode_vibe(crate::EncodeVibeRequest {
                        image: normalized_image,
                        information_extracted: image.info_extracted,
                        model: image.model,
                        strict_mode: false,
                    })
                    .await?;
                if let Ok(mut cache) = self.vibe_cache.lock() {
                    cache.insert(cache_key, encoded_vibe.clone());
                }
                encoded_vibe
            };

            encoded.push(vibe);
            strengths.push(image.strength);
            info_extracted.push(image.info_extracted);
        }

        Ok(Some((encoded, strengths, info_extracted)))
    }
}

fn normalize_u32_range(
    _field_name: &str,
    value: u32,
    min: u32,
    max: u32,
    strict_mode: bool,
    range_message: &'static str,
) -> Result<u32, BridgeError> {
    if strict_mode && !(min..=max).contains(&value) {
        return Err(BridgeError::invalid_request(range_message));
    }
    Ok(value.clamp(min, max))
}

fn normalize_f32_range(
    field_name: &str,
    value: f32,
    min: f32,
    max: f32,
    strict_mode: bool,
    range_message: &'static str,
) -> Result<f32, BridgeError> {
    if !value.is_finite() {
        return Err(BridgeError::invalid_request(format!(
            "{field_name} must be finite."
        )));
    }
    if strict_mode && !(min..=max).contains(&value) {
        return Err(BridgeError::invalid_request(range_message));
    }
    Ok(value.clamp(min, max))
}

fn normalize_image_dimension(value: u32, strict_mode: bool) -> Result<u32, BridgeError> {
    if strict_mode && (!(64..=1600).contains(&value) || !value.is_multiple_of(64)) {
        return Err(BridgeError::invalid_request(
            "size width/height must be in [64,1600] and multiple of 64.",
        ));
    }

    let clamped = value.clamp(64, 1600);
    let snapped = ((clamped + 32) / 64) * 64;
    Ok(snapped.clamp(64, 1600))
}
