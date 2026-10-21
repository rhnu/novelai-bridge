use crate::{
    director::{DirectorCommand, RunDirectorToolRequest},
    generation::test_support::image,
};

#[test]
fn operation_normalizes_before_encoding_and_exposes_effective_command() {
    let prepared = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::Colorize {
            prompt: Some("  blue  ".to_owned()),
            defry: 9,
        },
        policy: crate::ValidationPolicy::Normalize,
    }
    .prepare()
    .unwrap();
    assert!(
        matches!(prepared.command(), DirectorCommand::Colorize { prompt: Some(prompt), defry: 5 } if prompt == "blue")
    );
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "colorize");
    assert_eq!(body["defry"], 5);
    assert_eq!(body["prompt"], "blue");
    assert_eq!(body["width"], prepared.image_size().width);
    assert_eq!(
        prepared.adjustments(),
        &[crate::ValidationAdjustment {
            field: crate::RequestField::DirectorDefry,
            from: 9.0,
            to: 5.0
        }]
    );
    assert!(!format!("{prepared:?}").contains("blue"));
}

#[test]
fn emotion_requires_text_and_strict_defry_rejects_out_of_range_values() {
    for (command, strict) in [
        (
            DirectorCommand::Emotion {
                prompt: "   ".to_owned(),
                defry: 0,
            },
            false,
        ),
        (
            DirectorCommand::Colorize {
                prompt: None,
                defry: 6,
            },
            true,
        ),
    ] {
        assert!(
            RunDirectorToolRequest {
                image: image(),
                command,
                policy: if strict {
                    crate::ValidationPolicy::Reject
                } else {
                    crate::ValidationPolicy::Normalize
                }
            }
            .prepare()
            .is_err()
        );
    }
}

#[test]
fn parameterless_command_omits_prompt_and_defry() {
    let prepared = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::BgRemoval,
        policy: crate::ValidationPolicy::Reject,
    }
    .prepare()
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "bg-removal");
    assert!(body.get("prompt").is_none());
    assert!(body.get("defry").is_none());
}

#[test]
fn declutter_can_preserve_speech_bubbles_without_prompt_parameters() {
    let command = DirectorCommand::DeclutterKeepBubbles;
    assert_eq!(
        serde_json::from_value::<DirectorCommand>(serde_json::to_value(&command).unwrap()).unwrap(),
        command
    );
    let prepared = RunDirectorToolRequest {
        image: image(),
        command,
        policy: crate::ValidationPolicy::Reject,
    }
    .prepare()
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(body["req_type"], "declutter-keep-bubbles");
    assert!(body.get("prompt").is_none());
    assert!(body.get("defry").is_none());
}

#[test]
fn encoded_director_pixels_and_quote_use_the_same_processing_dimensions() {
    use base64::Engine as _;
    for (width, height, expected_width, expected_height) in
        [(1000, 1011, 1018, 1029), (3000, 1200, 2803, 1121)]
    {
        let pixels = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            width,
            height,
            image::Rgba([50, 100, 150, 255]),
        ));
        let png = crate::PreparedImage::from_pixels(&pixels).unwrap();
        let prepared = RunDirectorToolRequest {
            image: crate::EncodedImage::from_bytes(png.as_bytes()).unwrap(),
            command: DirectorCommand::Lineart,
            policy: crate::ValidationPolicy::Reject,
        }
        .prepare()
        .unwrap();
        assert_eq!(prepared.source_size(), crate::ImageSize { width, height });
        assert_eq!(
            prepared.image_size(),
            crate::ImageSize {
                width: expected_width,
                height: expected_height
            }
        );
        let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
        assert_eq!(body["width"], expected_width);
        assert_eq!(body["height"], expected_height);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body["image"].as_str().unwrap())
            .unwrap();
        let output = crate::EncodedImage::from_bytes(bytes).unwrap();
        assert_eq!(output.size(), prepared.image_size());
        // Flooring the target dimensions must not introduce fit padding around the source.
        let pixels = output.pixels().to_rgba8();
        assert_eq!(pixels.get_pixel(0, 0).0, [50, 100, 150, 255]);
        assert_eq!(
            pixels.get_pixel(expected_width - 1, expected_height - 1).0,
            [50, 100, 150, 255]
        );
        assert_eq!(
            prepared.estimate_anlas_cost(crate::AnlasPricingContext::default()),
            if width == 1000 { 20 } else { 60 }
        );
    }
}

#[test]
fn a_custom_director_preprocessor_must_return_the_processing_dimensions() {
    struct OriginalSize;
    impl crate::ImagePreprocessor for OriginalSize {
        fn prepare(
            &self,
            _field: crate::RequestField,
            image: &crate::EncodedImage,
            _operation: crate::ImagePreparation,
        ) -> Result<crate::PreparedImage, crate::BridgeError> {
            crate::PreparedImage::from_pixels(image.pixels())
        }
    }
    let error = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::Lineart,
        policy: crate::ValidationPolicy::Reject,
    }
    .prepare_with(&OriginalSize)
    .unwrap_err();
    assert!(
        matches!(error, crate::BridgeError::Preparation(crate::PreparationError::ImageSizeMismatch {
        field: crate::RequestField::Image,
        expected,
        ..
    }) if expected == crate::ImageSize::square())
    );
}

#[test]
fn removed_policy_and_invalid_command_fields_cannot_be_silently_ignored() {
    let request = RunDirectorToolRequest {
        image: image(),
        command: DirectorCommand::BgRemoval,
        policy: crate::ValidationPolicy::Reject,
    };
    let mut value = serde_json::to_value(request).unwrap();
    value.as_object_mut().unwrap().remove("policy");
    value["strict_mode"] = true.into();
    assert!(serde_json::from_value::<RunDirectorToolRequest>(value).is_err());
    assert!(
        serde_json::from_value::<DirectorCommand>(
            serde_json::json!({"colorize":{"prompt":null,"defry":1,"noise":0.5}})
        )
        .is_err()
    );
}
