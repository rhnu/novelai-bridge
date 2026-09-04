#![cfg(all(feature = "client", feature = "streaming"))]

mod support;

use novelai_bridge::{
    AnlasPricingContext, BridgeError, Character, CharacterPosition, CharacterReference, Client,
    EncodedImage, GenerateImageRequest, ImageSize, Img2ImgRequest, InvalidRequest, Model,
    NoiseSchedule, NormalizationChange, NormalizationPolicy, QualityPreset, StreamMode,
    VibeEncoding, VibeReference, VibeTransferConfig,
};

fn image() -> EncodedImage {
    EncodedImage::from_base64(&support::create_png_base64(2, 2)).unwrap()
}

fn request() -> GenerateImageRequest {
    GenerateImageRequest::builder("private prompt", Model::NaiDiffusion45Full)
        .seed(42)
        .build()
}

#[test]
fn stream_request_exposes_one_borrowed_base_view() {
    let stream = novelai_bridge::GenerateImageStreamRequest {
        base: request(),
        ..Default::default()
    };
    assert!(std::ptr::eq(stream.base(), stream.as_ref()));
}

fn character(index: usize) -> Character {
    Character {
        prompt: format!("character {index}"),
        negative_prompt: None,
        enabled: true,
        position: CharacterPosition::default(),
    }
}

#[test]
fn explicit_policy_overrides_strict_mode_and_raw_helpers_validate() {
    let mut raw = request();
    raw.strict_mode = true;
    raw.core.size.width = 2048;
    assert!(raw.clone().normalize().is_err());
    assert!(raw.resolve_prompts().is_err());
    assert!(
        raw.estimate_anlas_cost(AnlasPricingContext::default())
            .is_err()
    );
    let normalized = raw.normalize_with(NormalizationPolicy::Normalize).unwrap();
    assert!(!normalized.request().strict_mode);
    assert_eq!(normalized.policy(), NormalizationPolicy::Normalize);
    assert_eq!(normalized.request().core.size.width, 1600);

    let mut raw = request();
    raw.sampling.steps = 0;
    assert!(
        raw.clone()
            .normalize_with(NormalizationPolicy::Reject)
            .is_err()
    );
    assert_eq!(raw.normalize().unwrap().request().sampling.steps, 1);
}

#[test]
fn furry_mode_normalization_reports_v3_capability_drop() {
    let raw = GenerateImageRequest::builder("cat", Model::NaiDiffusion3)
        .furry_mode(true)
        .build();
    let normalized = raw.normalize().unwrap();

    assert!(!normalized.request().core.furry_mode);
    assert!(matches!(
        normalized.diagnostics(),
        [diagnostic]
            if diagnostic.field == "furry_mode"
                && diagnostic.change == NormalizationChange::Unsupported
    ));
}

#[test]
fn normalization_is_idempotent_and_diagnostics_are_deterministic() {
    let mut raw = request();
    raw.core.size = ImageSize {
        width: 2048,
        height: 1,
    };
    raw.core.n_samples = 9;
    raw.sampling.steps = 100;
    raw.sampling.scale = -2.0;
    raw.sampling.cfg_rescale = 2.0;
    raw.sampling.noise_schedule = NoiseSchedule::Native;
    raw.core.quality = QualityPreset::Light;
    raw.core.transparent_background = true;
    raw.guidance.smea = true;
    let first = raw.clone().normalize().unwrap();
    let second = raw.normalize().unwrap();
    assert_eq!(first, second);
    let changes: Vec<_> = first
        .diagnostics()
        .iter()
        .map(|item| item.field.as_str())
        .collect();
    assert_eq!(
        changes,
        [
            "steps",
            "scale",
            "n_samples",
            "cfg_rescale",
            "size.width",
            "size.height",
            "quality.light",
            "transparent_background",
            "guidance.smea",
            "sampling.noise_schedule",
        ]
    );
    assert!(matches!(
        first.diagnostics()[0].change,
        NormalizationChange::Adjusted {
            from: 100.0,
            to: 50.0,
        }
    ));
    assert_eq!(
        first
            .estimate_anlas_cost(AnlasPricingContext::default())
            .requested_samples,
        4
    );
    assert!(std::ptr::eq(
        first.resolved_prompts(),
        first.resolved_prompts()
    ));

    let effective = first.clone().into_request().normalize().unwrap();
    assert_eq!(effective.request(), first.request());
    assert_eq!(effective.resolved_prompts(), first.resolved_prompts());
    assert!(effective.diagnostics().is_empty());
    let mut edited = first.request().clone();
    edited.core.prompt = "different".to_owned();
    assert_ne!(edited.core.prompt, first.request().core.prompt);
}

#[test]
fn disabled_and_overflow_characters_keep_original_indices_but_are_not_validated() {
    let mut characters: Vec<_> = (0..9).map(character).collect();
    characters[0].enabled = false;
    characters[0].prompt.clear();
    characters[0].position.x = f32::NAN;
    characters[7].prompt.clear(); // Overflow must be dropped before checking its prompt.
    characters[8].enabled = false;
    let mut raw = request();
    raw.characters = Some(novelai_bridge::CharacterSet {
        characters,
        use_coords: None,
    });
    assert!(
        raw.clone()
            .normalize_with(NormalizationPolicy::Reject)
            .is_err()
    );
    let normalized = raw.normalize().unwrap();
    let indices: Vec<_> = normalized
        .resolved_prompts()
        .characters
        .iter()
        .map(|c| c.index)
        .collect();
    assert_eq!(indices, [1, 2, 3, 4, 5, 6]);
    assert!(normalized.diagnostics().iter().any(|item| matches!(
        item.change,
        NormalizationChange::CharacterLimit { index: 7, limit: 6 }
    )));
    let set = normalized.request().characters.as_ref().unwrap();
    assert_eq!(set.characters.len(), 9);
    assert!(set.characters[0].position.x.is_nan());
    assert!(!set.characters[7].enabled);
    assert!(
        normalized
            .into_request()
            .normalize_with(NormalizationPolicy::Reject)
            .is_ok()
    );
}

#[tokio::test]
async fn effective_coordinate_grid_matches_wire_and_remains_idempotent() {
    for explicit in [None, Some(false), Some(true)] {
        let mut raw = request();
        let mut ch = character(0);
        ch.position = CharacterPosition { x: 0.49, y: 1.5 };
        raw.characters = Some(novelai_bridge::CharacterSet {
            characters: vec![ch],
            use_coords: explicit,
        });
        let normalized = raw.normalize().unwrap();
        let set = normalized.request().characters.as_ref().unwrap();
        let expected = if explicit == Some(false) {
            CharacterPosition { x: 0.5, y: 0.5 }
        } else {
            CharacterPosition { x: 0.5, y: 0.9 }
        };
        assert_eq!(set.characters[0].position, expected);
        assert_eq!(set.use_coords, Some(explicit != Some(false)));
        let again = normalized.clone().into_request().normalize().unwrap();
        assert_eq!(normalized.request(), again.request());
        assert!(again.diagnostics().is_empty());
        let transport = support::MockTransport::default();
        Client::with_transport(transport.clone())
            .generate_prepared(normalized.prepare().unwrap())
            .await
            .unwrap();
        let body = transport.last_body().unwrap();
        let center = &body["parameters"]["characterPrompts"][0]["center"];
        assert!((center["x"].as_f64().unwrap() - f64::from(expected.x)).abs() < 1e-6);
        assert!((center["y"].as_f64().unwrap() - f64::from(expected.y)).abs() < 1e-6);
    }
}

#[tokio::test]
async fn v5_freeform_coordinates_survive_normalization_and_wire_serialization() {
    let expected = CharacterPosition { x: 0.681, y: 0.723 };
    let mut raw = GenerateImageRequest::builder("private prompt", Model::NaiDiffusion5Full)
        .seed(42)
        .build();
    let mut ch = character(0);
    ch.position = expected;
    raw.characters = Some(novelai_bridge::CharacterSet {
        characters: vec![ch],
        use_coords: Some(true),
    });

    let normalized = raw.normalize().unwrap();
    let set = normalized.request().characters.as_ref().unwrap();
    assert_eq!(set.characters[0].position, expected);
    assert!(normalized.diagnostics().is_empty());
    let again = normalized.clone().into_request().normalize().unwrap();
    assert_eq!(normalized.request(), again.request());
    assert!(again.diagnostics().is_empty());

    let transport = support::MockTransport::default();
    Client::with_transport(transport.clone())
        .generate_prepared(normalized.prepare().unwrap())
        .await
        .unwrap();
    let body = transport.last_body().unwrap();
    let center = &body["parameters"]["characterPrompts"][0]["center"];
    assert!((center["x"].as_f64().unwrap() - f64::from(expected.x)).abs() < 1e-6);
    assert!((center["y"].as_f64().unwrap() - f64::from(expected.y)).abs() < 1e-6);
    let caption_center =
        &body["parameters"]["v4_prompt"]["caption"]["char_captions"][0]["centers"][0];
    assert!((caption_center["x"].as_f64().unwrap() - f64::from(expected.x)).abs() < 1e-6);
    assert!((caption_center["y"].as_f64().unwrap() - f64::from(expected.y)).abs() < 1e-6);
}

#[test]
fn conflict_diagnostics_contain_field_names_not_payloads() {
    let mut raw = request();
    raw.references = Some(novelai_bridge::ReferenceSet {
        character_references: vec![CharacterReference {
            image: image(),
            reference_type: novelai_bridge::CharacterReferenceType::Character,
            fidelity: 1.0,
            strength: 1.0,
        }],
        vibe_transfer: Some(VibeTransferConfig {
            references: vec![VibeReference {
                vibe_data_cache: VibeEncoding::from_bytes(b"private vibe".to_vec()).unwrap(),
                strength: 1.0,
            }],
            strength: 1.0,
        }),
    });
    assert!(
        raw.clone()
            .normalize_with(NormalizationPolicy::Reject)
            .is_err()
    );
    let normalized = raw.normalize().unwrap();
    assert_eq!(normalized.diagnostics().len(), 1);
    assert!(matches!(
        normalized.diagnostics()[0].change,
        NormalizationChange::Conflict {
            retained_field: "character_references"
        }
    ));
    assert_eq!(normalized.diagnostics()[0].field, "vibe_transfer");
    let diagnostics = format!("{:?}", normalized.diagnostics());
    assert!(!diagnostics.contains("private"));
    assert_eq!(
        normalized
            .anlas_estimate_input(AnlasPricingContext::default())
            .character_reference_count,
        1
    );
}

#[test]
fn planning_and_pricing_preserve_validated_images_and_do_not_resolve_random_seed() {
    let mut raw = request();
    raw.core.seed = 0;
    raw.img2img = Some(Img2ImgRequest::new(image(), 2.0, -1.0));
    let normalized = raw.normalize().unwrap();
    assert_eq!(normalized.request().core.seed, 0);
    let cost = normalized.anlas_estimate_input(AnlasPricingContext::default());
    assert_eq!(cost.img2img_strength, Some(0.99));
    assert!(normalized.prepare().is_ok());
}

#[test]
fn img2img_and_inpaint_strength_use_separate_profiles() {
    let mut img2img = request();
    img2img.img2img = Some(Img2ImgRequest::new(image(), 1.0, 0.0));
    assert!(img2img.normalize_with(NormalizationPolicy::Reject).is_err());

    let mut inpaint_input = Img2ImgRequest::new(image(), 1.0, 0.0);
    inpaint_input.mask = Some(image());
    let mut inpaint = request();
    inpaint.img2img = Some(inpaint_input);
    let accepted = inpaint
        .clone()
        .normalize_with(NormalizationPolicy::Reject)
        .unwrap();
    assert_eq!(accepted.request().img2img.as_ref().unwrap().strength, 1.0);
    assert!(accepted.diagnostics().is_empty());

    inpaint.img2img.as_mut().unwrap().strength = 1.1;
    assert!(
        inpaint
            .clone()
            .normalize_with(NormalizationPolicy::Reject)
            .is_err()
    );
    let normalized = inpaint
        .normalize_with(NormalizationPolicy::Normalize)
        .unwrap();
    assert_eq!(normalized.request().img2img.as_ref().unwrap().strength, 1.0);
    assert!(normalized.diagnostics().iter().any(|item| {
        item.field == "img2img.strength"
            && matches!(
                item.change,
                NormalizationChange::Adjusted { from, to }
                    if (from - 1.1).abs() < 1e-6 && (to - 1.0).abs() < 1e-6
            )
    }));
}

#[tokio::test]
async fn preparation_resolves_seed_once_for_ordinary_and_streaming_send() {
    for seed in [0, 42] {
        let mut raw = request();
        raw.core.seed = seed;
        for streaming in [false, true] {
            let prepared = raw.clone().normalize().unwrap().prepare().unwrap();
            let resolved = prepared.resolved_seed();
            assert_eq!(prepared.normalized().request().core.seed, seed);
            if seed != 0 {
                assert_eq!(resolved, seed);
            }
            let transport = support::MockTransport::default();
            let client = Client::with_transport(transport.clone());
            let sent_seed = if streaming {
                client
                    .generate_stream_prepared(prepared, StreamMode::default())
                    .await
                    .unwrap()
                    .resolved_seed
            } else {
                client
                    .generate_prepared(prepared)
                    .await
                    .unwrap()
                    .resolved_seed
            };
            assert_eq!(sent_seed, resolved);
            assert_eq!(
                transport.last_body().unwrap()["parameters"]["seed"],
                resolved
            );
        }
    }
}

#[tokio::test]
async fn strict_rejection_precedes_image_decoding_and_transport() {
    let mut raw = request();
    raw.strict_mode = true;
    raw.core.size.width = 2048;
    raw.img2img = Some(Img2ImgRequest::new(image(), 0.5, 0.0));
    let transport = support::MockTransport::default();
    let result = Client::with_transport(transport.clone())
        .generate(raw)
        .await;
    assert!(matches!(
        result,
        Err(BridgeError::InvalidRequest(
            InvalidRequest::InvalidImageDimension { .. }
        ))
    ));
    assert!(transport.last_body().is_none());
}

#[tokio::test]
async fn prepared_v3_request_cannot_be_sent_as_stream() {
    let raw = GenerateImageRequest::builder("cat", Model::NaiDiffusion3).build();
    let transport = support::MockTransport::default();
    let result = Client::with_transport(transport.clone())
        .generate_stream_prepared(
            raw.normalize().unwrap().prepare().unwrap(),
            StreamMode::default(),
        )
        .await;
    assert!(matches!(
        result,
        Err(BridgeError::InvalidRequest(
            InvalidRequest::UnsupportedModelFeature { .. }
        ))
    ));
    assert!(transport.last_body().is_none());
}
