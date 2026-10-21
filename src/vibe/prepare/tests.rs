use crate::{EncodeVibeRequest, Model, generation::test_support::image};

fn sized_image(width: u32, height: u32) -> crate::EncodedImage {
    let prepared =
        crate::PreparedImage::from_pixels(&image::DynamicImage::new_rgba8(width, height)).unwrap();
    crate::EncodedImage::from_bytes(prepared.as_bytes()).unwrap()
}

#[derive(Default)]
struct RecordingPreprocessor(std::sync::Mutex<Vec<(crate::RequestField, crate::ImagePreparation)>>);

impl crate::ImagePreprocessor for RecordingPreprocessor {
    fn prepare(
        &self,
        field: crate::RequestField,
        image: &crate::EncodedImage,
        operation: crate::ImagePreparation,
    ) -> Result<crate::PreparedImage, crate::BridgeError> {
        self.0.lock().unwrap().push((field, operation));
        crate::ImagePreprocessor::prepare(&crate::DefaultImagePreprocessor, field, image, operation)
    }
}

#[test]
fn encoding_preparation_validates_capability_and_numeric_policy() {
    for (model, strength, strict, valid) in [
        (Model::NaiDiffusion45Full, 2.0, false, true),
        (Model::NaiDiffusion45Full, 2.0, true, false),
        (Model::NaiDiffusion45Full, f32::NAN, false, false),
        (Model::NaiDiffusion3, 0.5, false, false),
        (Model::NaiDiffusion5Full, 0.5, false, false),
    ] {
        let mut request = EncodeVibeRequest::new(image(), model);
        request.information_extracted = strength;
        request.policy = if strict {
            crate::ValidationPolicy::Reject
        } else {
            crate::ValidationPolicy::Normalize
        };
        let prepared = request.prepare();
        assert_eq!(prepared.is_ok(), valid);
        if let Ok(prepared) = prepared {
            let body: serde_json::Value =
                serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
            assert_eq!(prepared.model(), model);
            assert_eq!(body["model"], model.as_str());
            assert_eq!(body["information_extracted"], 1.0);
            assert_eq!(prepared.information_extracted(), 1.0);
            assert_eq!(
                prepared.adjustments(),
                &[crate::ValidationAdjustment {
                    field: crate::RequestField::VibeInformationExtracted,
                    from: 2.0,
                    to: 1.0
                }]
            );
        }
    }
}

#[test]
fn removed_strict_policy_cannot_silently_downgrade_vibe_validation() {
    let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
    request.information_extracted = 0.5;
    request.policy = crate::ValidationPolicy::Reject;
    let mut value = serde_json::to_value(request).unwrap();
    value.as_object_mut().unwrap().remove("policy");
    value["strict_mode"] = true.into();
    assert!(serde_json::from_value::<EncodeVibeRequest>(value).is_err());
}

#[test]
fn constructor_defaults_preserve_source_and_omit_unspecified_provider_options() {
    let source = sized_image(4, 3);
    let original = source.to_base64();
    let request = EncodeVibeRequest::new(source, Model::NaiDiffusion45Full);
    assert_eq!(request.policy, crate::ValidationPolicy::Normalize);
    assert_eq!(request.information_extracted, 1.0);
    let adapter = RecordingPreprocessor::default();
    let prepared = request.prepare_with(&adapter).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "image": original,
            "model": "nai-diffusion-4-5-full",
            "information_extracted": 1.0,
        })
    );
    assert!(adapter.0.lock().unwrap().is_empty());
    assert_eq!(prepared.adjustments(), []);
    assert!(!format!("{prepared:?}").contains(&original));
}

#[test]
fn zero_information_extraction_is_preserved_under_both_policies() {
    for policy in [
        crate::ValidationPolicy::Normalize,
        crate::ValidationPolicy::Reject,
    ] {
        let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
        request.policy = policy;
        request.information_extracted = 0.0;
        let prepared = request.prepare().unwrap();
        assert_eq!(prepared.information_extracted(), 0.0);
        assert_eq!(prepared.adjustments(), []);
        let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
        assert_eq!(body["information_extracted"], 0.0);
    }
}

#[test]
fn extraction_lower_bound_is_zero_and_nonfinite_values_always_fail() {
    let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
    request.information_extracted = -0.5;
    let normalized = request.clone().prepare().unwrap();
    assert_eq!(normalized.information_extracted(), 0.0);
    assert_eq!(
        normalized.adjustments(),
        &[crate::ValidationAdjustment {
            field: crate::RequestField::VibeInformationExtracted,
            from: -0.5,
            to: 0.0,
        }]
    );
    request.policy = crate::ValidationPolicy::Reject;
    assert!(matches!(
        request.prepare(),
        Err(crate::BridgeError::InvalidRequest(
            crate::InvalidRequest::NumericOutOfRange {
                field: crate::RequestField::VibeInformationExtracted,
                ..
            }
        ))
    ));
    for policy in [
        crate::ValidationPolicy::Normalize,
        crate::ValidationPolicy::Reject,
    ] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
            request.policy = policy;
            request.information_extracted = value;
            assert!(matches!(
                request.prepare(),
                Err(crate::BridgeError::InvalidRequest(
                    crate::InvalidRequest::NonFiniteNumber {
                        field: crate::RequestField::VibeInformationExtracted,
                        ..
                    }
                ))
            ));
        }
    }
}

#[test]
fn mask_is_prepared_for_the_source_canvas_and_all_explicit_options_survive_encoding() {
    use base64::Engine as _;
    let source = sized_image(3, 5);
    let original = source.to_base64();
    let mut request = EncodeVibeRequest::new(source, Model::NaiDiffusion45Full);
    request.information_extracted = 0.0;
    request.mask = Some(sized_image(4, 2));
    request.crop_to_mask = Some(true);
    request.focus_seed = Some(i64::MIN);
    request.info_extract_seed = Some(i64::MAX);
    let adapter = RecordingPreprocessor::default();
    let prepared = request.prepare_with(&adapter).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    let size = crate::ImageSize {
        width: 3,
        height: 5,
    };
    assert_eq!(prepared.image_size(), size);
    assert_eq!(
        *adapter.0.lock().unwrap(),
        vec![(
            crate::RequestField::VibeMask,
            crate::ImagePreparation::Mask(size)
        )]
    );
    assert_eq!(body["image"], original);
    assert_eq!(body["crop_to_mask"], true);
    assert_eq!(body["focus_seed"], i64::MIN);
    assert_eq!(body["info_extract_seed"], i64::MAX);
    let mask_bytes = base64::engine::general_purpose::STANDARD
        .decode(body["mask"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        crate::EncodedImage::from_bytes(mask_bytes).unwrap().size(),
        size
    );
    assert_eq!(prepared.encode().unwrap(), prepared.encode().unwrap());
}

#[test]
fn requested_crop_requires_a_mask_but_explicit_false_and_zero_seeds_are_preserved() {
    for policy in [
        crate::ValidationPolicy::Normalize,
        crate::ValidationPolicy::Reject,
    ] {
        let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
        request.policy = policy;
        request.crop_to_mask = Some(true);
        assert!(
            matches!(request.clone().prepare(), Err(crate::BridgeError::InvalidRequest(crate::InvalidRequest::RequiredFieldForContext { field: crate::RequestField::VibeMask, context })) if context == "crop_to_mask")
        );
        request.crop_to_mask = Some(false);
        request.focus_seed = Some(0);
        request.info_extract_seed = Some(-1);
        let prepared = request.prepare().unwrap();
        let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
        assert!(body.get("mask").is_none());
        assert_eq!(body["crop_to_mask"], false);
        assert_eq!(body["focus_seed"], 0);
        assert_eq!(body["info_extract_seed"], -1);
    }
}

#[test]
fn optional_encode_groups_reject_unknown_editable_fields() {
    let mut request = EncodeVibeRequest::new(image(), Model::NaiDiffusion45Full);
    request.mask = Some(image());
    request.crop_to_mask = Some(true);
    request.focus_seed = Some(0);
    request.info_extract_seed = Some(-1);
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(
        serde_json::from_value::<EncodeVibeRequest>(value.clone()).unwrap(),
        request
    );
    let mut unknown = value;
    unknown["unknown_option"] = serde_json::json!(true);
    assert!(serde_json::from_value::<EncodeVibeRequest>(unknown).is_err());
}

#[test]
fn relaxed_image_and_mask_constructors_cannot_bypass_default_preparation_budgets() {
    let policy = crate::ImageDecodePolicy {
        max_width: 17_000,
        ..Default::default()
    };
    let large = crate::PreparedImage::from_pixels_with_policy(
        &image::DynamicImage::new_rgba8(17_000, 1),
        policy,
    )
    .unwrap();
    let large = crate::EncodedImage::from_bytes_with_policy(large.as_bytes(), policy).unwrap();
    for (source, mask, expected_field) in [
        (large.clone(), None, crate::RequestField::Image),
        (image(), Some(large), crate::RequestField::VibeMask),
    ] {
        let mut request = EncodeVibeRequest::new(source, Model::NaiDiffusion45Full);
        request.mask = mask;
        let adapter = RecordingPreprocessor::default();
        assert!(
            matches!(request.prepare_with(&adapter), Err(crate::BridgeError::InvalidRequest(crate::InvalidRequest::UndecodableImage { field, .. })) if field == expected_field)
        );
        assert!(adapter.0.lock().unwrap().is_empty());
    }
}
