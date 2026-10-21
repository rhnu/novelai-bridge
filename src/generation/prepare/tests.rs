use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    BridgeError, DefaultImagePreprocessor, EncodedImage, GenerationInput, ImagePreparation,
    ImagePreprocessor, ImageSize, ImageToImageInput, InpaintImageToImage, InpaintingInput, Model,
    PreparedImage, RequestField, StreamMode,
    generation::{
        Character, CharacterPosition, CharacterSet, GenerateImageRequest, GenerationDelivery,
        test_support::image,
    },
};

fn payload(
    prepared: &super::PreparedGeneration,
    delivery: GenerationDelivery,
) -> serde_json::Value {
    serde_json::from_slice(&prepared.encode(delivery).unwrap()).unwrap()
}

#[test]
fn legacy_uc_is_a_v4_negative_condition_option_with_typed_model_gating() {
    for model in Model::ALL {
        let request = GenerateImageRequest::builder("cat", model)
            .seed(42)
            .legacy_uc(true)
            .build();
        let supported = matches!(
            model,
            Model::NaiDiffusion4Full | Model::NaiDiffusion4Curated
        );
        assert_eq!(model.descriptor().prompt().legacy_uc, supported);
        let strict = request
            .clone()
            .normalize_with(crate::ValidationPolicy::Reject);
        if supported {
            for inpaint in [false, true] {
                let mut request = request.clone();
                if inpaint {
                    request.operation =
                        GenerationInput::Inpainting(InpaintingInput::new(image(), image()));
                }
                let prepared = request
                    .normalize_with(crate::ValidationPolicy::Reject)
                    .unwrap()
                    .prepare()
                    .unwrap();
                let body = payload(&prepared, GenerationDelivery::Images);
                assert_eq!(body["parameters"]["v4_negative_prompt"]["legacy_uc"], true);
                assert!(body["parameters"].get("legacy_uc").is_none());
            }
            assert!(strict.is_ok());
        } else {
            assert!(
                matches!(strict, Err(BridgeError::InvalidRequest(crate::InvalidRequest::UnsupportedModelFeature { feature, .. })) if feature == crate::GenerationField::LegacyUc.into())
            );
            let plan = request.normalize().unwrap();
            assert!(!plan.request().core.legacy_uc);
            assert!(
                plan.diagnostics()
                    .iter()
                    .any(|item| item.field == crate::GenerationField::LegacyUc)
            );
        }
    }
    let prepared = GenerateImageRequest::builder("cat", Model::NaiDiffusion4Full)
        .seed(42)
        .build()
        .normalize()
        .unwrap()
        .prepare()
        .unwrap();
    assert_eq!(
        payload(&prepared, GenerationDelivery::Images)["parameters"]["v4_negative_prompt"]["legacy_uc"],
        false
    );
}

#[test]
fn one_preparation_reuses_images_and_seed_for_both_deliveries() {
    #[derive(Default)]
    struct Counter(AtomicUsize);
    impl ImagePreprocessor for Counter {
        fn prepare(
            &self,
            field: RequestField,
            image: &EncodedImage,
            operation: ImagePreparation,
        ) -> Result<PreparedImage, BridgeError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            DefaultImagePreprocessor.prepare(field, image, operation)
        }
    }
    let mut input = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .build();
    input.operation = GenerationInput::ImageToImage(ImageToImageInput::new(image(), 0.5, 0.0));
    let counter = Counter::default();
    let prepared = input.normalize().unwrap().prepare_with(&counter).unwrap();
    assert_ne!(prepared.resolved_seed(), 0);
    let ordinary = payload(&prepared, GenerationDelivery::Images);
    let stream = payload(&prepared, GenerationDelivery::Stream(StreamMode::Sse));
    assert_eq!(ordinary["parameters"]["seed"], prepared.resolved_seed());
    assert_eq!(
        ordinary["parameters"]["extra_noise_seed"],
        prepared.resolved_seed()
    );
    assert_eq!(stream["parameters"]["seed"], ordinary["parameters"]["seed"]);
    assert_eq!(
        stream["parameters"]["image"],
        ordinary["parameters"]["image"]
    );
    assert!(ordinary["parameters"].get("stream").is_none());
    assert_eq!(stream["parameters"]["stream"], "sse");
    let binary = payload(
        &prepared,
        GenerationDelivery::Stream(StreamMode::MessagePack),
    );
    assert_eq!(binary["parameters"]["stream"], "msgpack");
    let mut comparable = binary;
    comparable["parameters"]["stream"] = "sse".into();
    assert_eq!(comparable, stream);
    assert_eq!(counter.0.load(Ordering::Relaxed), 1);
}

#[test]
fn fixed_seed_is_preserved_and_unsupported_delivery_is_rejected() {
    let prepared = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
        .seed(42)
        .build()
        .normalize()
        .unwrap()
        .prepare()
        .unwrap();
    assert_eq!(prepared.resolved_seed(), 42);
    assert!(prepared.encode(GenerationDelivery::Images).is_ok());
    assert!(
        prepared
            .encode(GenerationDelivery::Stream(StreamMode::Sse))
            .is_err()
    );
    assert!(
        prepared
            .encode(GenerationDelivery::Stream(StreamMode::MessagePack))
            .is_err()
    );
}

#[test]
fn character_geometry_is_shared_by_all_wire_representations() {
    for (model, position, expected) in [
        (
            Model::NaiDiffusion45Full,
            CharacterPosition { x: 0.49, y: 1.0 },
            CharacterPosition { x: 0.5, y: 0.9 },
        ),
        (
            Model::NaiDiffusion5Full,
            CharacterPosition { x: 0.681, y: 0.723 },
            CharacterPosition { x: 0.681, y: 0.723 },
        ),
    ] {
        let mut input = GenerateImageRequest::builder("cat", model).seed(42).build();
        input.characters = Some(CharacterSet {
            characters: vec![Character {
                prompt: "character".to_owned(),
                negative_prompt: Some("negative".to_owned()),
                position,
                enabled: true,
            }],
            use_coords: Some(true),
        });
        let plan = input.normalize().unwrap();
        assert_eq!(plan.resolved_prompts().characters[0].position, expected);
        let prepared = plan.prepare().unwrap();
        let body = payload(&prepared, GenerationDelivery::Images);
        let center = &body["parameters"]["characterPrompts"][0]["center"];
        let caption_center =
            &body["parameters"]["v4_prompt"]["caption"]["char_captions"][0]["centers"][0];
        assert_eq!(center, caption_center);
        assert!((center["x"].as_f64().unwrap() - f64::from(expected.x)).abs() < 1e-6);
        assert!((center["y"].as_f64().unwrap() - f64::from(expected.y)).abs() < 1e-6);
    }
}

#[test]
fn wire_versions_select_required_and_omitted_fields() {
    for (model, version, structured, extensions) in [
        (Model::NaiDiffusion3, 4, false, false),
        (Model::NaiDiffusion45Full, 4, true, false),
        (Model::NaiDiffusion5Full, 4, true, true),
    ] {
        let prepared = GenerateImageRequest::builder("cat", model)
            .seed(42)
            .build()
            .normalize()
            .unwrap()
            .prepare()
            .unwrap();
        let body = payload(&prepared, GenerationDelivery::Images);
        let parameters = &body["parameters"];
        assert_eq!(parameters["params_version"], version);
        assert_eq!(parameters.get("v4_prompt").is_some(), structured);
        assert_eq!(parameters.get("prompt").is_some(), !structured);
        assert_eq!(parameters.get("legacy_uc").is_some(), extensions);
    }
}

#[test]
fn pure_inpainting_requires_a_mask_and_omits_source_conditioning() {
    let source = image();
    let input = InpaintingInput::new(source.clone(), source);
    let mut serialized = serde_json::to_value(&input).unwrap();
    serialized["extra_noise_seed"] = serde_json::json!(123);
    assert!(serde_json::from_value::<InpaintingInput>(serialized).is_err());
    let mut missing_mask = serde_json::to_value(&input).unwrap();
    missing_mask.as_object_mut().unwrap().remove("mask");
    assert!(serde_json::from_value::<InpaintingInput>(missing_mask).is_err());

    let prepared = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .seed(42)
        .inpainting(input)
        .build()
        .normalize()
        .unwrap()
        .prepare()
        .unwrap();
    let body = payload(&prepared, GenerationDelivery::Images);
    let parameters = &body["parameters"];
    assert_eq!(body["action"], "infill");
    assert!(parameters["mask"].is_string());
    assert!(parameters.get("img2img").is_none());
    assert!(parameters.get("inpaintImg2ImgStrength").is_none());
    assert!(parameters.get("noise").is_none());
    assert!(parameters.get("extra_noise_seed").is_none());
    assert!(parameters.get("color_correct").is_none());
    assert!(parameters.get("strength").is_none());
}

#[test]
fn explicit_inpaint_conditioning_is_nested_and_preserved_at_full_strength() {
    for (extra_noise_seed, expected_seed) in [(None, 42), (Some(73), 73)] {
        let mut input = InpaintingInput::new(image(), image());
        input.img2img = Some(InpaintImageToImage {
            strength: 1.0,
            noise: 0.25,
            color_correct: false,
            extra_noise_seed,
        });
        let prepared = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
            .seed(42)
            .size(ImageSize {
                width: 64,
                height: 64,
            })
            .inpainting(input)
            .build()
            .normalize()
            .unwrap()
            .prepare()
            .unwrap();
        let ordinary = payload(&prepared, GenerationDelivery::Images);
        let stream = payload(&prepared, GenerationDelivery::Stream(StreamMode::Sse));
        let parameters = &ordinary["parameters"];
        assert_eq!(
            parameters["img2img"],
            serde_json::json!({
                "strength": 1.0,
                "noise": 0.25,
                "color_correct": false,
                "extra_noise_seed": expected_seed,
            })
        );
        assert_eq!(stream["parameters"]["img2img"], parameters["img2img"]);
        for name in [
            "strength",
            "noise",
            "color_correct",
            "extra_noise_seed",
            "inpaintImg2ImgStrength",
        ] {
            assert!(parameters.get(name).is_none());
        }
    }
}

#[test]
fn max_enhance_wire_and_source_use_input_dimensions_while_quotes_use_their_target() {
    use base64::Engine as _;
    let pixels = image::DynamicImage::new_rgba8(1001, 777);
    let png = PreparedImage::from_pixels(&pixels).unwrap();
    let source = EncodedImage::from_bytes(png.as_bytes()).unwrap();
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .seed(42)
        .max_enhance(ImageToImageInput::new(source, 0.5, 0.0))
        .build();
    assert_eq!(
        request.core.size,
        ImageSize {
            width: 1001,
            height: 777
        }
    );
    let plan = request.normalize().unwrap();
    let geometry = *plan.geometry();
    let basis = plan.anlas_estimate_input(crate::AnlasPricingContext::default());
    assert_eq!(
        basis.size,
        ImageSize {
            width: 2048,
            height: 1536
        }
    );
    let prepared = plan.prepare().unwrap();
    assert_eq!(prepared.geometry(), &geometry);
    assert_eq!(
        geometry.output_size,
        ImageSize {
            width: 2048,
            height: 1536
        }
    );
    for delivery in [
        GenerationDelivery::Images,
        GenerationDelivery::Stream(StreamMode::Sse),
    ] {
        let body = payload(&prepared, delivery);
        assert_eq!(body["parameters"]["width"], 1024);
        assert_eq!(body["parameters"]["height"], 768);
        assert_eq!(body["parameters"]["upscaled_enhance"], true);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body["parameters"]["image"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            EncodedImage::from_bytes(bytes).unwrap().size(),
            geometry.input_size
        );
    }
    assert_eq!(
        prepared
            .normalized()
            .anlas_estimate_input(crate::AnlasPricingContext::default()),
        basis
    );
}

#[test]
fn character_reference_projection_keeps_each_records_values_together() {
    use crate::{CharacterReference, CharacterReferenceType};
    let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .seed(42)
        .character_references(vec![
            CharacterReference {
                image: image(),
                reference_type: CharacterReferenceType::Style,
                information_extracted: 0.25,
                fidelity: -0.25,
                strength: -0.5,
            },
            CharacterReference {
                image: image(),
                reference_type: CharacterReferenceType::Character,
                information_extracted: 0.75,
                fidelity: 2.0,
                strength: 1.75,
            },
        ])
        .build();
    let prepared = input.normalize().unwrap().prepare().unwrap();
    let body = payload(&prepared, GenerationDelivery::Images);
    let parameters = &body["parameters"];
    assert_eq!(
        parameters["director_reference_images"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for (index, caption, secondary, strength) in
        [(0, "style", 1.25, -0.5), (1, "character", -1.0, 1.75)]
    {
        assert_eq!(
            parameters["director_reference_descriptions"][index]["caption"]["base_caption"],
            caption
        );
        assert_eq!(
            parameters["director_reference_secondary_strength_values"][index],
            secondary
        );
        assert_eq!(
            parameters["director_reference_strength_values"][index],
            strength
        );
        assert_eq!(
            parameters["director_reference_information_extracted"][index],
            if index == 0 { 0.25 } else { 0.75 }
        );
    }
}

#[test]
fn encoded_vibe_preparation_projects_the_single_normalization_without_rescaling() {
    use crate::{VibeEncoding, VibeReference, VibeTransferConfig};
    let references = [-1.400_443_8, -0.591_067_5, 3.574_618_3]
        .map(|strength| {
            VibeReference::encoded(VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap(), strength)
        })
        .to_vec();
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion4Full)
        .seed(42)
        .vibe_transfer(VibeTransferConfig::new(references))
        .build()
        .normalize()
        .unwrap();
    let expected: Vec<_> = plan
        .request()
        .references
        .as_ref()
        .unwrap()
        .vibe_transfer
        .as_ref()
        .unwrap()
        .references
        .iter()
        .map(|reference| reference.strength)
        .collect();
    let prepared = plan.prepare().unwrap();
    for delivery in [
        GenerationDelivery::Images,
        GenerationDelivery::Stream(StreamMode::Sse),
    ] {
        assert_eq!(
            serde_json::from_value::<Vec<f32>>(
                payload(&prepared, delivery)["parameters"]["reference_strength_multiple"].clone()
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn raw_and_encoded_vibes_project_distinct_parallel_arrays_without_controlnet() {
    use crate::{VibeEncoding, VibeReference, VibeTransferConfig};
    for raw in [false, true] {
        let references = if raw {
            vec![
                VibeReference::image(image(), 0.25, -2.0),
                VibeReference::image(image(), 0.75, 4.0),
            ]
        } else {
            vec![
                VibeReference::encoded(VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap(), -2.0),
                VibeReference::encoded(VibeEncoding::from_bytes(vec![4, 5, 6]).unwrap(), 4.0),
            ]
        };
        let model = if raw {
            Model::NaiDiffusion3
        } else {
            Model::NaiDiffusion4Full
        };
        let plan = GenerateImageRequest::builder("cat", model)
            .seed(42)
            .vibe_transfer(VibeTransferConfig::new(references))
            .build()
            .normalize()
            .unwrap();
        let parameters =
            payload(&plan.prepare().unwrap(), GenerationDelivery::Images)["parameters"].clone();
        assert!(parameters.get("controlnet_strength").is_none());
        assert_eq!(
            parameters["reference_image_multiple"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        if raw {
            assert!(
                parameters
                    .get("normalize_reference_strength_multiple")
                    .is_none()
            );
            assert_eq!(
                parameters["reference_information_extracted_multiple"],
                serde_json::json!([0.25, 0.75])
            );
            assert_eq!(
                parameters["reference_strength_multiple"],
                serde_json::json!([-2.0, 4.0])
            );
            for image in parameters["reference_image_multiple"].as_array().unwrap() {
                let bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    image.as_str().unwrap(),
                )
                .unwrap();
                assert_eq!(
                    EncodedImage::from_bytes(bytes).unwrap().size(),
                    ImageSize {
                        width: 448,
                        height: 448
                    }
                );
            }
        } else {
            assert_eq!(parameters["normalize_reference_strength_multiple"], true);
            assert!(
                parameters
                    .get("reference_information_extracted_multiple")
                    .is_none()
            );
            assert_eq!(
                parameters["reference_image_multiple"],
                serde_json::json!(["AQID", "BAUG"])
            );
            let strengths = parameters["reference_strength_multiple"]
                .as_array()
                .unwrap();
            assert!((strengths[0].as_f64().unwrap() + 1.0 / 3.0).abs() < 0.000_001);
            assert!((strengths[1].as_f64().unwrap() - 2.0 / 3.0).abs() < 0.000_001);
        }
    }
    let bare = GenerateImageRequest::builder("cat", Model::NaiDiffusion4Full)
        .seed(42)
        .build()
        .normalize()
        .unwrap()
        .prepare()
        .unwrap();
    assert!(
        payload(&bare, GenerationDelivery::Images)["parameters"]
            .get("controlnet_strength")
            .is_none()
    );
}

#[test]
fn transparent_output_preserves_both_alpha_representations_and_omits_opaque_options() {
    use crate::AlphaMode;
    for (model, transparent, mode, expected) in [
        (
            Model::NaiDiffusion5Full,
            true,
            AlphaMode::Straight,
            Some(true),
        ),
        (
            Model::NaiDiffusion5Full,
            true,
            AlphaMode::Premultiplied,
            Some(false),
        ),
        (
            Model::NaiDiffusion5FullMedium,
            true,
            AlphaMode::Straight,
            Some(true),
        ),
        (Model::NaiDiffusion5Full, false, AlphaMode::Straight, None),
        (
            Model::NaiDiffusion45Full,
            true,
            AlphaMode::Premultiplied,
            None,
        ),
    ] {
        let prepared = GenerateImageRequest::builder("cat", model)
            .transparent_background(transparent)
            .alpha_mode(mode)
            .seed(42)
            .build()
            .normalize()
            .unwrap()
            .prepare()
            .unwrap();
        let body = payload(&prepared, GenerationDelivery::Images);
        assert_eq!(
            body["parameters"]
                .get("straight_alpha")
                .and_then(serde_json::Value::as_bool),
            expected
        );
    }
}

#[test]
fn debug_summaries_do_not_expose_prompt_or_prepared_image_content() {
    let source = image();
    let private_image = source.to_base64();
    let input = GenerateImageRequest::builder("private-positive-marker", Model::NaiDiffusion45Full)
        .negative_prompt("private-negative-marker")
        .characters(vec![Character {
            prompt: "private-character-marker".to_owned(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: true,
        }])
        .image_to_image(ImageToImageInput::new(source, 0.5, 0.0))
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .seed(42)
        .build();
    let raw_debug = format!("{input:?}");
    let normalized = input.normalize().unwrap();
    let normalized_debug = format!("{normalized:?}");
    let prepared = normalized.prepare().unwrap();
    let prepared_debug = format!("{prepared:?}");
    for summary in [&raw_debug, &normalized_debug, &prepared_debug] {
        for marker in [
            "private-positive-marker",
            "private-negative-marker",
            "private-character-marker",
            private_image.as_str(),
        ] {
            assert!(!summary.contains(marker));
        }
    }
    assert!(prepared_debug.contains("NaiDiffusion45Full"));
}
