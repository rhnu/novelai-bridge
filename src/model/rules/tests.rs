use super::*;
use crate::{Model, ReferenceModes};

#[test]
fn reference_profiles_cannot_enable_a_missing_inpainting_sibling() {
    let mut descriptor = Model::NaiDiffusion45Full.descriptor().clone();
    descriptor.support.inpainting = None;
    assert!(
        descriptor
            .model_id_for(GenerationMode::Inpainting)
            .is_none()
    );
    let query = descriptor.resolve_references(
        GenerationMode::Inpainting,
        ReferenceIntent {
            vibe: true,
            character: true,
        },
    );
    assert_eq!(query.character, ReferenceAvailability::UnsupportedMode);
    assert_eq!(query.vibe, ReferenceAvailability::UnsupportedMode);
}

#[test]
fn mode_support_is_checked_before_reference_conflict_precedence() {
    let mut descriptor = Model::NaiDiffusion45Full.descriptor().clone();
    descriptor
        .support
        .character_reference
        .as_mut()
        .unwrap()
        .modes = ReferenceModes::Unmasked;
    descriptor.support.vibe.as_mut().unwrap().modes = ReferenceModes::All;
    let query = descriptor.resolve_references(
        GenerationMode::Inpainting,
        ReferenceIntent {
            vibe: true,
            character: true,
        },
    );
    assert_eq!(query.character, ReferenceAvailability::UnsupportedMode);
    assert_eq!(query.vibe, ReferenceAvailability::Available);
}

#[test]
fn unsupported_character_reference_does_not_suppress_implemented_vibe() {
    let descriptor = Model::NaiDiffusion4Full.descriptor();
    let result = descriptor.resolve_references(GenerationMode::TextToImage, ReferenceIntent { vibe: true, character: true });
    assert_eq!(result.character, ReferenceAvailability::Unsupported);
    assert_eq!(result.vibe, ReferenceAvailability::Available);
}

#[test]
fn implemented_character_reference_takes_precedence_over_vibe() {
    let descriptor = Model::NaiDiffusion45Full.descriptor();
    let result = descriptor.resolve_references(GenerationMode::TextToImage, ReferenceIntent { vibe: true, character: true });
    assert_eq!(result.character, ReferenceAvailability::Available);
    assert_eq!(result.vibe, ReferenceAvailability::ConflictWithCharacterReference);
}

#[test]
fn provider_only_input_is_distinct_from_absent_capability() {
    let legacy = Model::NaiDiffusion3.descriptor();
    let current = Model::NaiDiffusion5Full.descriptor();
    let intent = ReferenceIntent { vibe: true, character: false };
    assert_eq!(legacy.resolve_references(GenerationMode::TextToImage, intent).vibe, ReferenceAvailability::InputNotImplemented);
    assert_eq!(current.resolve_references(GenerationMode::TextToImage, intent).vibe, ReferenceAvailability::Unsupported);
    assert!(!legacy.can_encode_vibe());
}

#[test]
fn generation_modes_cannot_create_an_absent_character_reference_capability() {
    for model in [Model::NaiDiffusion5Full, Model::NaiDiffusion5Curated] {
        for mode in [GenerationMode::TextToImage, GenerationMode::ImageToImage, GenerationMode::Inpainting] {
            let availability = model.descriptor().resolve_references(mode, ReferenceIntent { vibe: false, character: true });
            assert_eq!(availability.character, ReferenceAvailability::Unsupported);
        }
    }
}
