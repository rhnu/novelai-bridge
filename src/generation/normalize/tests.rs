use crate::{
    BridgeError, CharacterField, GenerationField, InpaintImageToImageField, InvalidRequest, Model,
    NoiseSchedule, ValidationPolicy, VibeEncoding, VibeReference, VibeTransferConfig,
    generation::{
        Character, CharacterPosition, CharacterReference, CharacterReferenceType, CharacterSet,
        GenerateImageRequest, GenerationInput, ImageToImageInput, InpaintImageToImage,
        InpaintingInput, NormalizationChange, ReferenceSet, test_support::image,
    },
};

fn request() -> GenerateImageRequest {
    GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .seed(42)
        .build()
}

fn character(index: usize) -> Character {
    Character {
        prompt: format!("character {index}"),
        negative_prompt: None,
        position: CharacterPosition::default(),
        enabled: true,
    }
}

#[test]
fn normalization_is_deterministic_and_idempotent() {
    let mut input = request();
    input.core.size.width = 2048;
    input.core.n_samples = 9;
    input.sampling.steps = 100;
    input.sampling.noise_schedule = NoiseSchedule::Native;
    let first = input.clone().normalize().unwrap();
    let second = input.normalize().unwrap();
    assert_eq!(first, second);
    assert!(
        first
            .diagnostics()
            .iter()
            .any(|entry| entry.field == GenerationField::Steps)
    );
    let repeated = first.clone().into_request().normalize().unwrap();
    assert_eq!(repeated.request(), first.request());
    assert_eq!(repeated.resolved_prompts(), first.resolved_prompts());
    assert_eq!(repeated.diagnostics(), []);
}

#[test]
fn explicit_policy_overrides_editable_policy_and_reports_exact_paths() {
    let mut input = request();
    input.policy = ValidationPolicy::Reject;
    input.sampling.steps = 0;
    assert!(input.clone().normalize().is_err());
    let plan = input.normalize_with(ValidationPolicy::Normalize).unwrap();
    assert_eq!(plan.policy(), ValidationPolicy::Normalize);
    assert_eq!(plan.request().sampling.steps, 1);
    assert_eq!(plan.diagnostics()[0].field, GenerationField::Steps);
    assert_eq!(plan.diagnostics()[0].field.to_string(), "sampling.steps");
    assert_eq!(
        GenerationField::Character {
            index: 2,
            field: CharacterField::X
        }
        .to_string(),
        "characters.characters[2].position.x"
    );
}

#[test]
fn disabled_and_overflow_entries_do_not_validate_and_keep_input_indices() {
    let mut characters: Vec<_> = (0..9).map(character).collect();
    characters[0].enabled = false;
    characters[0].prompt.clear();
    characters[0].position.x = f32::NAN;
    characters[7].prompt.clear();
    characters[8].enabled = false;
    let mut input = request();
    input.characters = Some(CharacterSet::new(characters));
    assert!(
        input
            .clone()
            .normalize_with(ValidationPolicy::Reject)
            .is_err()
    );
    let plan = input.normalize().unwrap();
    assert_eq!(
        plan.resolved_prompts()
            .characters
            .iter()
            .map(|entry| entry.index)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    assert!(plan.diagnostics().iter().any(|entry| matches!(
        entry.change,
        NormalizationChange::CharacterLimit { index: 7, limit: 6 }
    )));
    assert!(
        plan.into_request()
            .normalize_with(ValidationPolicy::Reject)
            .is_ok()
    );
}

#[test]
fn finite_values_are_required_in_both_policies() {
    for policy in [ValidationPolicy::Reject, ValidationPolicy::Normalize] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut input = request();
            input.sampling.scale = value;
            assert!(matches!(
                input.normalize_with(policy),
                Err(BridgeError::InvalidRequest(
                    InvalidRequest::NonFiniteNumber { .. }
                ))
            ));
        }
    }
}

#[test]
fn disabled_characters_do_not_enable_coordinates_under_any_override() {
    for use_coords in [None, Some(false), Some(true)] {
        let mut disabled = character(0);
        disabled.enabled = false;
        disabled.prompt.clear();
        disabled.position.x = f32::NAN;
        let mut input = request();
        input.characters = Some(CharacterSet {
            characters: vec![disabled],
            use_coords,
        });
        let plan = input.normalize_with(ValidationPolicy::Reject).unwrap();
        assert!(!plan.resolved_prompts().use_coords);
        assert_eq!(plan.resolved_prompts().characters, []);
    }
}

#[test]
fn rejected_character_overflow_reports_only_the_enabled_count() {
    let mut characters: Vec<_> = (0..8).map(character).collect();
    characters[0].enabled = false;
    let mut input = request();
    input.characters = Some(CharacterSet::new(characters));
    let error = input.normalize_with(ValidationPolicy::Reject).unwrap_err();
    assert!(
        matches!(error, BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { field, value: 7.0, .. }) if field == GenerationField::Characters.into())
    );
}

#[test]
fn empty_vibe_is_always_absent_independent_of_other_reference_groups() {
    for character_present in [false, true] {
        for policy in [ValidationPolicy::Reject, ValidationPolicy::Normalize] {
            let mut input = request();
            input.references = Some(ReferenceSet {
                vibe_transfer: Some(VibeTransferConfig {
                    strength: f32::NAN,
                    references: Vec::new(),
                }),
                character_references: if character_present {
                    vec![CharacterReference {
                        image: image(),
                        reference_type: CharacterReferenceType::Character,
                        fidelity: 1.0,
                        strength: 1.0,
                    }]
                } else {
                    Vec::new()
                },
            });
            let plan = input.normalize_with(policy).unwrap();
            assert_eq!(plan.request().references.is_some(), character_present);
            assert!(
                plan.request()
                    .references
                    .as_ref()
                    .is_none_or(|references| references.vibe_transfer.is_none())
            );
        }
    }
}

#[test]
fn reference_conflict_uses_typed_fields_and_never_contains_payload_text() {
    let input = request_with_references();
    assert!(
        input
            .clone()
            .normalize_with(ValidationPolicy::Reject)
            .is_err()
    );
    let plan = input.normalize().unwrap();
    assert!(
        matches!(plan.diagnostics(), [entry] if entry.field == GenerationField::VibeTransfer && matches!(entry.change, NormalizationChange::Conflict { retained_field: GenerationField::CharacterReferences }))
    );
    assert!(!format!("{:?}", plan.diagnostics()).contains("private"));
}

fn request_with_references() -> GenerateImageRequest {
    let mut input = request();
    input.references = Some(ReferenceSet {
        character_references: vec![CharacterReference {
            image: image(),
            reference_type: CharacterReferenceType::Character,
            fidelity: 1.0,
            strength: 1.0,
        }],
        vibe_transfer: Some(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 1.0,
                vibe_data_cache: VibeEncoding::from_bytes(b"private".to_vec()).unwrap(),
            }],
        }),
    });
    input
}

#[test]
fn image_and_mask_strengths_use_separate_bounds() {
    let mut ordinary = request();
    ordinary.operation = GenerationInput::ImageToImage(ImageToImageInput::new(image(), 1.0, 0.0));
    assert!(ordinary.normalize_with(ValidationPolicy::Reject).is_err());
    let mut masked = request();
    let mut inpainting = InpaintingInput::new(image(), image());
    inpainting.img2img = Some(InpaintImageToImage::new(1.0, 0.0));
    masked.operation = GenerationInput::Inpainting(inpainting);
    assert!(
        masked
            .clone()
            .normalize_with(ValidationPolicy::Reject)
            .is_ok()
    );
    let GenerationInput::Inpainting(input) = &mut masked.operation else {
        unreachable!()
    };
    input.img2img.as_mut().unwrap().strength = 1.1;
    let plan = masked.normalize().unwrap();
    assert!(plan.diagnostics().iter().any(|entry| entry.field
        == GenerationField::InpaintImageToImage(InpaintImageToImageField::Strength)));
}

#[test]
fn inpaint_conditioning_ranges_and_diagnostics_use_nested_field_locations() {
    for (strength, noise, field, path) in [
        (
            0.0,
            0.0,
            InpaintImageToImageField::Strength,
            "operation.input.img2img.strength",
        ),
        (
            0.5,
            1.5,
            InpaintImageToImageField::Noise,
            "operation.input.img2img.noise",
        ),
    ] {
        let mut input = InpaintingInput::new(image(), image());
        input.img2img = Some(InpaintImageToImage::new(strength, noise));
        let mut request = request();
        request.operation = GenerationInput::Inpainting(input);
        let expected = GenerationField::InpaintImageToImage(field);
        assert_eq!(expected.to_string(), path);
        assert!(matches!(
            request.clone().normalize_with(ValidationPolicy::Reject),
            Err(BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { field, .. }))
                if field == expected.into()
        ));
        let normalized = request.normalize().unwrap();
        assert!(
            normalized
                .diagnostics()
                .iter()
                .any(|entry| entry.field == expected)
        );
    }
}

#[test]
fn inpaint_conditioning_rejects_nonfinite_nested_values_under_both_policies() {
    for (strength, noise, expected) in [
        (f32::NAN, 0.0, InpaintImageToImageField::Strength),
        (0.5, f32::INFINITY, InpaintImageToImageField::Noise),
    ] {
        for policy in [ValidationPolicy::Normalize, ValidationPolicy::Reject] {
            let mut input = InpaintingInput::new(image(), image());
            input.img2img = Some(InpaintImageToImage::new(strength, noise));
            let mut request = request();
            request.operation = GenerationInput::Inpainting(input);
            assert!(matches!(
                request.normalize_with(policy),
                Err(BridgeError::InvalidRequest(InvalidRequest::NonFiniteNumber { field, .. }))
                    if field == GenerationField::InpaintImageToImage(expected).into()
            ));
        }
    }
}

#[test]
fn legacy_inpainting_drops_or_rejects_the_complete_source_conditioning_group() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let mut input = InpaintingInput::new(image(), image());
        input.img2img = Some(InpaintImageToImage::new(0.5, 0.2));
        let request = GenerateImageRequest::builder("cat", model)
            .inpainting(input)
            .build();
        let expected = GenerationField::InpaintImageToImage(InpaintImageToImageField::Entry);
        assert_eq!(expected.to_string(), "operation.input.img2img");
        assert!(matches!(
            request.clone().normalize_with(ValidationPolicy::Reject),
            Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
                if feature == expected.into()
        ));
        let plan = request.normalize().unwrap();
        let GenerationInput::Inpainting(input) = &plan.request().operation else {
            unreachable!()
        };
        assert!(input.img2img.is_none());
        assert!(
            plan.diagnostics()
                .iter()
                .any(|entry| entry.field == expected
                    && matches!(entry.change, NormalizationChange::Unsupported))
        );
        assert_eq!(
            plan.anlas_estimate_input(crate::AnlasPricingContext::default())
                .mode,
            crate::PricingMode::Inpainting { strength: 1.0 }
        );
    }
}

#[test]
fn unsupported_noise_schedule_obeys_the_same_policy_as_other_capabilities() {
    let mut input = request();
    input.sampling.noise_schedule = NoiseSchedule::Native;
    let error = input
        .clone()
        .normalize_with(ValidationPolicy::Reject)
        .unwrap_err();
    assert!(
        matches!(error, BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. })
        if feature == GenerationField::NoiseSchedule.into())
    );
    let normalized = input.normalize_with(ValidationPolicy::Normalize).unwrap();
    assert_eq!(
        normalized.request().sampling.noise_schedule,
        NoiseSchedule::Karras
    );
    assert!(
        matches!(normalized.diagnostics(), [entry] if entry.field == GenerationField::NoiseSchedule
        && matches!(entry.change, NormalizationChange::Fallback))
    );
}
