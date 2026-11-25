use crate::{
    BridgeError, CharacterReference, CharacterReferenceField, CharacterReferenceType,
    GenerateImageRequest, GenerationField, InpaintingInput, InvalidRequest, Model,
    NormalizationChange, ValidationPolicy, VibeEncoding, VibeReference, VibeReferenceField,
    VibeTransferConfig, generation::test_support::image,
};

fn encoded(strength: f32) -> VibeReference {
    VibeReference::encoded(VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap(), strength)
}

fn request(model: Model, references: Vec<VibeReference>, normalize: bool) -> GenerateImageRequest {
    let mut config = VibeTransferConfig::new(references);
    config.normalize_strength = normalize;
    GenerateImageRequest::builder("cat", model)
        .seed(42)
        .vibe_transfer(config)
        .build()
}

#[test]
fn encoded_strengths_follow_absolute_sum_with_signs_and_opt_out() {
    for (values, normalize, expected) in [
        (vec![2.0, -3.0], true, vec![0.4, -0.6]),
        (vec![2.0, -3.0], false, vec![2.0, -3.0]),
        (vec![-3.0], true, vec![-3.0]),
        (vec![0.0, 0.0], true, vec![0.0, 0.0]),
        (vec![0.25, -0.25], true, vec![0.25, -0.25]),
        (vec![f32::MAX, -f32::MAX], true, vec![0.5, -0.5]),
    ] {
        for policy in [ValidationPolicy::Normalize, ValidationPolicy::Reject] {
            let plan = request(
                Model::NaiDiffusion4Full,
                values.iter().copied().map(encoded).collect(),
                normalize,
            )
            .normalize_with(policy)
            .unwrap();
            let actual: Vec<_> = plan
                .request()
                .references
                .as_ref()
                .unwrap()
                .vibe_transfer
                .as_ref()
                .unwrap()
                .references
                .iter()
                .map(|item| item.strength)
                .collect();
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn normalization_of_effective_encoded_requests_is_idempotent() {
    for values in [
        vec![1.0, 1.0, 1.0],
        vec![0.7, 0.8, -0.2],
        vec![1.0, -2.0, 3.0, -4.0],
    ] {
        let first = request(
            Model::NaiDiffusion4Full,
            values.into_iter().map(encoded).collect(),
            true,
        )
        .normalize()
        .unwrap();
        let expected = first.request().clone();
        let second = first.into_request().normalize().unwrap();
        assert_eq!(second.request(), &expected);
        assert_eq!(second.diagnostics(), []);
    }
}

#[test]
fn raw_references_preserve_signed_strength_and_skip_encoded_normalization() {
    let plan = request(
        Model::NaiDiffusion3,
        vec![
            VibeReference::image(image(), 0.4, -2.0),
            VibeReference::image(image(), 0.8, 4.0),
        ],
        true,
    )
    .normalize_with(ValidationPolicy::Reject)
    .unwrap();
    let references = &plan
        .request()
        .references
        .as_ref()
        .unwrap()
        .vibe_transfer
        .as_ref()
        .unwrap()
        .references;
    assert_eq!(
        references.iter().map(|r| r.strength).collect::<Vec<_>>(),
        [-2.0, 4.0]
    );
}

#[test]
fn wrong_input_paths_are_rejected_or_dropped_with_original_row_locations() {
    for (model, refs) in [
        (
            Model::NaiDiffusion3,
            vec![encoded(1.0), VibeReference::image(image(), 2.0, 4.0)],
        ),
        (
            Model::NaiDiffusion4Full,
            vec![
                VibeReference::image(image(), 0.5, 1.0),
                encoded(2.0),
                encoded(-2.0),
            ],
        ),
    ] {
        let input = request(model, refs, true);
        let error = input
            .clone()
            .normalize_with(ValidationPolicy::Reject)
            .unwrap_err();
        assert!(
            matches!(error, BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }) if feature == GenerationField::VibeReference { index: 0, field: VibeReferenceField::Input }.into())
        );
        let normalized = input.normalize().unwrap();
        let expected_indices = if model == Model::NaiDiffusion3 {
            &[1][..]
        } else {
            &[1, 2][..]
        };
        assert_eq!(normalized.vibe_reference_source_indices(), expected_indices);
        assert_eq!(
            normalized.diagnostics()[0].field,
            GenerationField::VibeReference {
                index: 0,
                field: VibeReferenceField::Input
            }
        );
        assert!(normalized.diagnostics().iter().any(|d| d.field
            == GenerationField::VibeReference {
                index: 1,
                field: if model == Model::NaiDiffusion3 {
                    VibeReferenceField::InformationExtracted
                } else {
                    VibeReferenceField::Strength
                }
            }));
        let expected = normalized.request().clone();
        let repeated = normalized.into_request().normalize().unwrap();
        assert_eq!(repeated.request(), &expected);
        assert_eq!(repeated.diagnostics(), []);
    }
    let empty = request(Model::NaiDiffusion3, vec![encoded(1.0)], true)
        .normalize()
        .unwrap();
    assert!(empty.request().references.is_none());
}

#[test]
fn retained_reference_numbers_must_be_finite_under_both_policies() {
    for policy in [ValidationPolicy::Normalize, ValidationPolicy::Reject] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let error = request(Model::NaiDiffusion4Full, vec![encoded(value)], true)
                .normalize_with(policy)
                .unwrap_err();
            assert!(
                matches!(error, BridgeError::InvalidRequest(InvalidRequest::NonFiniteNumber { field, .. }) if field == GenerationField::VibeReference { index: 0, field: VibeReferenceField::Strength }.into())
            );
            let mut precise = CharacterReference::new(image(), CharacterReferenceType::Character);
            precise.fidelity = value;
            let error = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
                .character_references(vec![precise])
                .build()
                .normalize_with(policy)
                .unwrap_err();
            assert!(
                matches!(error, BridgeError::InvalidRequest(InvalidRequest::NonFiniteNumber { field, .. }) if field == GenerationField::CharacterReference { index: 0, field: CharacterReferenceField::Fidelity }.into())
            );
        }
    }
}

#[test]
fn precise_fidelity_and_strength_are_finite_signed_values() {
    let mut reference = CharacterReference::new(image(), CharacterReferenceType::Style);
    reference.fidelity = -2.0;
    reference.strength = 3.0;
    reference.information_extracted = 0.4;
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .character_references(vec![reference.clone()])
        .build()
        .normalize_with(ValidationPolicy::Reject)
        .unwrap();
    assert_eq!(
        plan.request()
            .references
            .as_ref()
            .unwrap()
            .character_references,
        [reference]
    );
    assert_eq!(plan.diagnostics(), []);
}

#[test]
fn v5_reference_fidelity_is_fixed_and_high_strength_is_retained() {
    let mut reference = CharacterReference::new(image(), CharacterReferenceType::Style);
    reference.fidelity = 0.25;
    reference.strength = -5.5;
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .character_references(vec![reference.clone()])
        .build();
    let field = GenerationField::CharacterReference {
        index: 0,
        field: CharacterReferenceField::Fidelity,
    };
    assert!(matches!(
        request.clone().normalize_with(ValidationPolicy::Reject),
        Err(BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { field: actual, .. }))
            if actual == field.into()
    ));
    let plan = request.normalize().unwrap();
    reference.fidelity = 1.0;
    assert_eq!(
        plan.request()
            .references
            .as_ref()
            .unwrap()
            .character_references,
        [reference]
    );
    assert_eq!(plan.diagnostics()[0].field, field);
    assert_eq!(plan.character_reference_source_indices(), [0]);
}

#[test]
fn v5_medium_uses_high_only_when_a_supported_reference_is_applied() {
    let build = |kind| {
        GenerateImageRequest::builder("cat", Model::NaiDiffusion5FullMedium)
            .character_references(vec![CharacterReference::new(image(), kind)])
            .build()
    };
    let with_reference = build(CharacterReferenceType::Character);
    assert!(matches!(
        with_reference.clone().normalize_with(ValidationPolicy::Reject),
        Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
            if feature == GenerationField::Model.into()
    ));
    let high = with_reference.normalize().unwrap();
    assert_eq!(high.model_id(), "nai-diffusion-5-full");
    assert_eq!(high.request().core.model, Model::NaiDiffusion5Full);
    assert_eq!(high.request().sampling.steps, 23);
    assert_eq!(high.character_reference_source_indices(), [0]);
    let medium = build(CharacterReferenceType::Delta).normalize().unwrap();
    assert_eq!(medium.model_id(), "nai-diffusion-5-full-medium");
    assert!(medium.character_reference_source_indices().is_empty());
}

#[test]
fn curated_inpainting_projects_compatible_references_and_retains_source_positions() {
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Curated)
        .inpainting(InpaintingInput::new(image(), image()))
        .character_references(
            [
                CharacterReferenceType::CharacterMainOutfit,
                CharacterReferenceType::Costume,
                CharacterReferenceType::CharacterAndStyleAnyOutfit,
                CharacterReferenceType::Style,
            ]
            .map(|kind| CharacterReference::new(image(), kind))
            .to_vec(),
        )
        .build()
        .normalize()
        .unwrap();
    assert_eq!(plan.model_id(), "nai-diffusion-4-5-curated-inpainting");
    assert_eq!(plan.character_reference_source_indices(), [0, 2, 3]);
    let references = &plan
        .request()
        .references
        .as_ref()
        .unwrap()
        .character_references;
    let types: Vec<_> = references
        .iter()
        .map(|entry| entry.reference_type)
        .collect();
    assert_eq!(
        types,
        [
            CharacterReferenceType::Character,
            CharacterReferenceType::CharacterAndStyle,
            CharacterReferenceType::Style,
        ]
    );
    for (index, change) in [
        (0, NormalizationChange::Fallback),
        (1, NormalizationChange::Unsupported),
        (2, NormalizationChange::Fallback),
    ] {
        assert!(plan.diagnostics().iter().any(|diagnostic| diagnostic.field
            == GenerationField::CharacterReference {
                index,
                field: CharacterReferenceField::ReferenceType,
            }
            && diagnostic.change == change));
    }
}

#[test]
fn native_v5_inpainting_preserves_refinement_and_drops_unsupported_references() {
    let mut input = InpaintingInput::new(image(), image());
    input.img2img = Some(crate::InpaintImageToImage::new(0.5, 0.1));
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5FullMedium)
        .inpainting(input)
        .character_references(vec![CharacterReference::new(
            image(),
            CharacterReferenceType::Character,
        )])
        .build();
    assert!(matches!(
        request.clone().normalize_with(ValidationPolicy::Reject),
        Err(BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { feature, .. }))
            if feature == GenerationField::CharacterReferences.into()
    ));
    let normalized = request.normalize().unwrap();
    assert_eq!(
        normalized.model_id(),
        "nai-diffusion-5-full-medium-inpainting"
    );
    assert!(normalized.request().references.is_none());
    assert!(
        matches!(&normalized.request().operation, crate::GenerationInput::Inpainting(input)
        if input.img2img.is_some())
    );
}

#[test]
fn unsupported_types_do_not_count_towards_limit_or_suppress_vibes() {
    let image = image();
    let valid = CharacterReference::new(image.clone(), CharacterReferenceType::Style);
    let unavailable = CharacterReference::new(image, CharacterReferenceType::Costume);
    let mut references = vec![unavailable.clone()];
    references.extend(std::iter::repeat_n(valid.clone(), 17));
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .character_references(references)
        .build()
        .normalize()
        .unwrap();
    assert_eq!(
        plan.character_reference_source_indices(),
        (1..=16).collect::<Vec<_>>()
    );
    assert!(plan.diagnostics().iter().any(|diagnostic| {
        diagnostic.field
            == GenerationField::CharacterReference {
                index: 17,
                field: CharacterReferenceField::Entry,
            }
            && diagnostic.change == NormalizationChange::ReferenceLimit { limit: 16 }
    }));
    let error = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .character_references(vec![valid; 17])
        .build()
        .normalize_with(ValidationPolicy::Reject)
        .unwrap_err();
    assert!(matches!(error,
        BridgeError::InvalidRequest(InvalidRequest::TooManyCharacterReferences { field, limit: 16 })
        if field == GenerationField::CharacterReference { index: 16, field: CharacterReferenceField::Entry }.into()
    ));
    let plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .character_references(vec![unavailable])
        .vibe_transfer(VibeTransferConfig::new(vec![encoded(1.0)]))
        .build()
        .normalize()
        .unwrap();
    assert_eq!(plan.vibe_reference_source_indices(), [0]);
    assert!(plan.character_reference_source_indices().is_empty());
}
