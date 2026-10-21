use serde_json::json;

use crate::{
    Character, CharacterPosition, CharacterReference, CharacterReferenceType, CharacterSet,
    GenerateImageRequest, GenerateImageStreamRequest, GenerationInput, GenerationMode,
    ImageToImageInput, InpaintImageToImage, InpaintingInput, Model, ReferenceSet, ValidationPolicy,
    VibeEncoding, VibeReference, VibeTransferConfig, generation::test_support::image,
};

fn request_with_nested_inputs() -> GenerateImageRequest {
    let mut request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full).build();
    request.characters = Some(CharacterSet::new(vec![Character {
        prompt: "cat".to_owned(),
        negative_prompt: None,
        position: CharacterPosition::default(),
        enabled: true,
    }]));
    request.references = Some(ReferenceSet {
        vibe_transfer: Some(VibeTransferConfig {
            references: vec![VibeReference {
                vibe_data_cache: VibeEncoding::from_bytes(vec![0, 1, 255]).unwrap(),
                strength: 0.5,
            }],
            strength: 0.5,
        }),
        character_references: vec![CharacterReference {
            image: image(),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.5,
        }],
    });
    request.operation = GenerationInput::ImageToImage(ImageToImageInput::new(image(), 0.5, 0.0));
    request
}

#[test]
fn editable_requests_reject_unknown_fields_at_every_nested_input_group() {
    for policy in [ValidationPolicy::Normalize, ValidationPolicy::Reject] {
        let mut request = request_with_nested_inputs();
        request.policy = policy;
        let serialized = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serde_json::from_value::<GenerateImageRequest>(serialized.clone()).unwrap(),
            request,
        );
        for path in [
            "",
            "/core",
            "/core/size",
            "/sampling",
            "/characters",
            "/characters/characters/0",
            "/characters/characters/0/position",
            "/guidance",
            "/operation",
            "/operation/input",
            "/references",
            "/references/character_references/0",
            "/references/vibe_transfer",
            "/references/vibe_transfer/references/0",
            "/output",
        ] {
            let mut value = serialized.clone();
            value.pointer_mut(path).unwrap()["unknown_option"] = json!(true);
            assert!(
                serde_json::from_value::<GenerateImageRequest>(value.clone()).is_err(),
                "{path}, {policy:?}",
            );
            let stream = json!({"base": value, "stream": "sse"});
            assert!(
                serde_json::from_value::<GenerateImageStreamRequest>(stream).is_err(),
                "stream {path}, {policy:?}",
            );
        }
        let mut value = serialized;
        value["output"]["straight_alpha"] = json!(true);
        assert!(serde_json::from_value::<GenerateImageRequest>(value).is_err());
    }
}

#[test]
fn inpainting_input_rejects_unknown_fields_inside_an_editable_request() {
    let mut request = request_with_nested_inputs();
    let mut input = InpaintingInput::new(image(), image());
    input.img2img = Some(InpaintImageToImage {
        strength: 0.5,
        noise: 0.2,
        color_correct: false,
        extra_noise_seed: Some(42),
    });
    request.operation = GenerationInput::Inpainting(input);
    for policy in [ValidationPolicy::Normalize, ValidationPolicy::Reject] {
        request.policy = policy;
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(
            serde_json::from_value::<GenerateImageRequest>(value.clone()).unwrap(),
            request,
        );
        for path in ["/operation/input", "/operation/input/img2img"] {
            let mut unknown = value.clone();
            unknown.pointer_mut(path).unwrap()["unknown"] = json!(true);
            assert!(serde_json::from_value::<GenerateImageRequest>(unknown).is_err());
        }
        for (field, invalid) in [
            ("strength", json!(0.5)),
            ("noise", json!(0.2)),
            ("color_correct", json!(true)),
            ("extra_noise_seed", json!(42)),
        ] {
            let mut flat = value.clone();
            flat["operation"]["input"][field] = invalid;
            assert!(serde_json::from_value::<GenerateImageRequest>(flat).is_err());
        }
    }
}

#[test]
fn operation_envelopes_reject_fields_that_would_discard_mode_specific_input() {
    for value in [
        json!({"mode": "text_to_image", "img2img": {}}),
        json!({"mode": "text_to_image", "mask": "ignored"}),
        json!({"mode": "text_to_image", "input": {"image": "ignored"}}),
        json!({"mode": "unknown"}),
    ] {
        assert!(serde_json::from_value::<GenerationInput>(value).is_err());
    }
    let mut value = serde_json::to_value(GenerationInput::ImageToImage(ImageToImageInput::new(
        image(),
        0.5,
        0.0,
    )))
    .unwrap();
    value["mask"] = json!("ignored");
    assert!(serde_json::from_value::<GenerationInput>(value).is_err());
}

#[test]
fn requests_reject_removed_image_and_validation_fields() {
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full).build();
    for (field, value) in [("img2img", json!({})), ("strict_mode", json!(true))] {
        let mut serialized = serde_json::to_value(&request).unwrap();
        serialized[field] = value;
        assert!(serde_json::from_value::<GenerateImageRequest>(serialized).is_err());
    }
}

#[test]
fn each_serialized_operation_preserves_its_explicit_mode_and_inputs() {
    for (operation, mode) in [
        (GenerationInput::TextToImage, GenerationMode::TextToImage),
        (
            GenerationInput::ImageToImage(ImageToImageInput::new(image(), 0.5, 0.0)),
            GenerationMode::ImageToImage,
        ),
        (
            GenerationInput::Inpainting(InpaintingInput::new(image(), image())),
            GenerationMode::Inpainting,
        ),
    ] {
        let encoded = serde_json::to_value(&operation).unwrap();
        let decoded: GenerationInput = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.mode(), mode);
        assert_eq!(decoded, operation);
    }
}
