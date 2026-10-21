use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    BridgeError, DefaultImagePreprocessor, EncodedImage, ImagePreparation, ImagePreprocessor,
    ImageSize, Img2ImgRequest, Model, PreparedImage, StreamMode,
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
            field: &str,
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
    input.img2img = Some(Img2ImgRequest::new(image(), 0.5, 0.0));
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
