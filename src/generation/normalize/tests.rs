use crate::{
    BridgeError, InvalidRequest, Model, NoiseSchedule, VibeEncoding, VibeReference,
    VibeTransferConfig,
    generation::{
        Character, CharacterField, CharacterPosition, CharacterReference, CharacterReferenceType,
        CharacterSet, GenerateImageRequest, GenerationField, ImageInputField, Img2ImgRequest,
        NormalizationChange, NormalizationPolicy, ReferenceSet, test_support::image,
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
    input.strict_mode = true;
    input.sampling.steps = 0;
    assert!(input.clone().normalize().is_err());
    let plan = input
        .normalize_with(NormalizationPolicy::Normalize)
        .unwrap();
    assert_eq!(plan.policy(), NormalizationPolicy::Normalize);
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
            .normalize_with(NormalizationPolicy::Reject)
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
            .normalize_with(NormalizationPolicy::Reject)
            .is_ok()
    );
}

#[test]
fn finite_values_are_required_in_both_policies() {
    for policy in [NormalizationPolicy::Reject, NormalizationPolicy::Normalize] {
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
fn empty_vibe_is_always_absent_independent_of_other_reference_groups() {
    for character_present in [false, true] {
        for policy in [NormalizationPolicy::Reject, NormalizationPolicy::Normalize] {
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
            .normalize_with(NormalizationPolicy::Reject)
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
    ordinary.img2img = Some(Img2ImgRequest::new(image(), 1.0, 0.0));
    assert!(
        ordinary
            .normalize_with(NormalizationPolicy::Reject)
            .is_err()
    );
    let mut masked = request();
    let mut source = Img2ImgRequest::new(image(), 1.0, 0.0);
    source.mask = Some(image());
    masked.img2img = Some(source);
    assert!(
        masked
            .clone()
            .normalize_with(NormalizationPolicy::Reject)
            .is_ok()
    );
    masked.img2img.as_mut().unwrap().strength = 1.1;
    let plan = masked.normalize().unwrap();
    assert!(
        plan.diagnostics()
            .iter()
            .any(|entry| entry.field == GenerationField::ImageInput(ImageInputField::Strength))
    );
}
