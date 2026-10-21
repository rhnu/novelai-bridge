use serde_json::json;

use crate::{
    GenerateImageRequest, GenerationInput, GenerationMode, ImageToImageInput, InpaintingInput,
    Model, generation::test_support::image,
};

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
            GenerationInput::Inpainting(InpaintingInput::new(image(), image(), 1.0, 0.0)),
            GenerationMode::Inpainting,
        ),
    ] {
        let encoded = serde_json::to_value(&operation).unwrap();
        let decoded: GenerationInput = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.mode(), mode);
        assert_eq!(decoded, operation);
    }
}
