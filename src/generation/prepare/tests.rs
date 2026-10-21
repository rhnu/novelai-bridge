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
        (Model::NaiDiffusion3, 3, false, false),
        (Model::NaiDiffusion45Full, 3, true, false),
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
fn character_reference_projection_keeps_each_records_values_together() {
    use crate::{CharacterReference, CharacterReferenceType};
    let input = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .seed(42)
        .character_references(vec![
            CharacterReference {
                image: image(),
                reference_type: CharacterReferenceType::Style,
                fidelity: 0.25,
                strength: 0.5,
            },
            CharacterReference {
                image: image(),
                reference_type: CharacterReferenceType::Character,
                fidelity: 1.0,
                strength: 0.75,
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
        [(0, "style", 0.75, 0.5), (1, "character", 0.0, 0.75)]
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
            1.0
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
