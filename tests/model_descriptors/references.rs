use super::MODES;
use novelai_bridge::{
    AnlasPricingContext, CharacterReference, CharacterReferenceType, EncodedImage,
    GenerateImageRequest, GenerationMode, Img2ImgRequest, Model, NormalizationPolicy,
    ReferenceAvailability, ReferenceIntent, ReferenceSet, VibeEncoding, VibeInputPath,
    VibeReference, VibeTransferConfig,
};

fn image() -> EncodedImage {
    EncodedImage::from_base64(&super::support::create_png_base64(2, 2)).unwrap()
}

fn request(model: Model, mode: GenerationMode, intent: ReferenceIntent) -> GenerateImageRequest {
    let mut raw = GenerateImageRequest::builder("cat", model).build();
    if mode != GenerationMode::TextToImage {
        let mut image = Img2ImgRequest::new(image(), 0.5, 0.0);
        if mode == GenerationMode::Inpainting {
            image.mask = Some(self::image());
        }
        raw.img2img = Some(image);
    }
    raw.references = Some(ReferenceSet {
        character_references: if intent.character {
            vec![CharacterReference {
                image: image(),
                reference_type: CharacterReferenceType::Character,
                fidelity: 1.0,
                strength: 1.0,
            }]
        } else {
            Vec::new()
        },
        vibe_transfer: intent.vibe.then(|| VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
                strength: 1.0,
            }],
        }),
    });
    raw
}

// Explicit expectations do not use the implementation's profiles to predict its own answers.
fn expected(
    model: Model,
    mode: GenerationMode,
    intent: ReferenceIntent,
) -> (ReferenceAvailability, ReferenceAvailability) {
    use ReferenceAvailability::{
        Available, ConflictWithCharacterReference, InputNotImplemented, NotRequested, Unsupported,
        UnsupportedMode,
    };
    let character = if intent.character {
        match model {
            Model::NaiDiffusion45Full | Model::NaiDiffusion45Curated => Available,
            _ => Unsupported,
        }
    } else {
        NotRequested
    };
    let vibe = if intent.vibe {
        match model {
            Model::NaiDiffusion3 | Model::NaiDiffusion3Furry => InputNotImplemented,
            Model::NaiDiffusion5Full | Model::NaiDiffusion5Curated => Unsupported,
            _ if character == Available => ConflictWithCharacterReference,
            _ if mode == GenerationMode::Inpainting => UnsupportedMode,
            _ => Available,
        }
    } else {
        NotRequested
    };
    (vibe, character)
}

#[test]
fn reference_query_and_both_normalization_policies_agree_for_all_96_combinations() {
    for model in Model::ALL {
        for mode in MODES {
            for vibe in [false, true] {
                for character in [false, true] {
                    let intent = ReferenceIntent { vibe, character };
                    let (expected_vibe, expected_character) = expected(model, mode, intent);
                    let query = model.descriptor().resolve_references(mode, intent);
                    assert_eq!(query.vibe, expected_vibe, "{model:?} {mode:?} {intent:?}");
                    assert_eq!(
                        query.character, expected_character,
                        "{model:?} {mode:?} {intent:?}"
                    );
                    let raw = request(model, mode, intent);
                    let reject = raw.clone().normalize_with(NormalizationPolicy::Reject);
                    assert_eq!(
                        reject.is_ok(),
                        (!vibe || query.vibe.is_available())
                            && (!character || query.character.is_available())
                    );
                    let normalized = raw.normalize().unwrap();
                    let retained = normalized.request().references.as_ref();
                    assert_eq!(
                        retained.is_some_and(|set| !set.character_references.is_empty()),
                        query.character.is_available()
                    );
                    assert_eq!(
                        retained.is_some_and(|set| set.vibe_transfer.is_some()),
                        query.vibe.is_available()
                    );
                    let pricing = normalized.anlas_estimate_input(AnlasPricingContext::default());
                    assert_eq!(
                        pricing.character_reference_count,
                        u32::from(query.character.is_available())
                    );
                    assert_eq!(
                        pricing.vibe_reference_count,
                        u32::from(query.vibe.is_available())
                    );
                    assert!(
                        normalized
                            .into_request()
                            .normalize()
                            .unwrap()
                            .diagnostics()
                            .is_empty()
                    );
                }
            }
        }
    }
}

#[test]
fn provider_only_vibe_is_distinct_from_absence_and_implemented_input() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let descriptor = model.descriptor();
        assert_eq!(
            descriptor.support().vibe.unwrap().input,
            VibeInputPath::ProviderOnlyLegacy
        );
        assert!(model.vibe_model_key().is_some());
        assert!(!descriptor.can_encode_vibe());
    }
    for model in [
        Model::NaiDiffusion4Full,
        Model::NaiDiffusion4Curated,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion45Curated,
    ] {
        assert_eq!(
            model.descriptor().support().vibe.unwrap().input,
            VibeInputPath::Encoded
        );
        assert!(model.descriptor().can_encode_vibe());
    }
    for model in [Model::NaiDiffusion5Full, Model::NaiDiffusion5Curated] {
        assert!(model.descriptor().support().vibe.is_none());
        assert!(model.descriptor().support().character_reference.is_none());
        assert!(model.vibe_model_key().is_none());
        assert!(!model.descriptor().can_encode_vibe());
    }
}
