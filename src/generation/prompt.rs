use crate::{
    BridgeError, Character, CharacterPosition, CharacterPositionProfile, CharacterReference,
    ImagePreparation, ImagePreprocessor, ResolvedCharacterPrompt,
    generation::wire::{
        CenterPoint, CharacterPrompt, ReferenceCaption, ReferenceConditionInput, V4CharacterCaption,
    },
};

const GRID_CENTER: f32 = 0.5;
const GRID_TOLERANCE: f32 = 0.0001;

fn center_point(
    position: CharacterPosition,
    use_coords: bool,
    positioning: CharacterPositionProfile,
) -> CenterPoint {
    if use_coords {
        CenterPoint {
            x: positioning.resolve_coordinate(position.x),
            y: positioning.resolve_coordinate(position.y),
        }
    } else {
        CenterPoint {
            x: GRID_CENTER,
            y: GRID_CENTER,
        }
    }
}

pub fn is_center_position(value: f32) -> bool {
    (value - GRID_CENTER).abs() <= GRID_TOLERANCE
}

/// Builds the V4 character captions from already-resolved prompt text.
///
/// Text comes exclusively from [`ResolvedCharacterPrompt`]; this helper only decides geometry and
/// which characters take part. Re-deriving text from [`Character`] here would reintroduce a second
/// prompt-assembly path.
pub fn build_v4_character_captions(
    characters: &[Character],
    resolved: &[ResolvedCharacterPrompt],
    negative: bool,
    use_coords: bool,
    positioning: CharacterPositionProfile,
) -> Vec<V4CharacterCaption> {
    let mut out = Vec::new();
    for resolved in resolved {
        let character = &characters[resolved.index];

        out.push(V4CharacterCaption {
            centers: Some(vec![center_point(
                character.position,
                use_coords,
                positioning,
            )]),
            char_caption: Some(if negative {
                resolved.negative_prompt.clone()
            } else {
                resolved.prompt.clone()
            }),
        });
    }

    out
}

/// Builds the `characterPrompts` wire entries from already-resolved prompt text.
///
/// Only enabled, resolved characters are sent.
pub fn convert_character_prompts(
    characters: &[Character],
    resolved: &[ResolvedCharacterPrompt],
    use_coords: bool,
    positioning: CharacterPositionProfile,
) -> Vec<CharacterPrompt> {
    resolved
        .iter()
        .map(|resolved| CharacterPrompt {
            prompt: resolved.prompt.clone(),
            uc: resolved.negative_prompt.clone(),
            center: center_point(characters[resolved.index].position, use_coords, positioning),
            enabled: true,
        })
        .collect()
}

pub struct CharacterReferenceWireParts {
    pub images: Vec<String>,
    pub descriptions: Vec<ReferenceConditionInput>,
    pub information_extracted: Vec<f32>,
    pub secondary_strength_values: Vec<f32>,
    pub strength_values: Vec<f32>,
}

pub fn convert_character_references(
    refs: &[CharacterReference],
    preprocessor: &dyn ImagePreprocessor,
) -> Result<CharacterReferenceWireParts, BridgeError> {
    let mut images = Vec::with_capacity(refs.len());
    let mut descriptions = Vec::with_capacity(refs.len());
    let mut information_extracted = Vec::with_capacity(refs.len());
    let mut secondary_strength_values = Vec::with_capacity(refs.len());
    let mut strength_values = Vec::with_capacity(refs.len());

    for (idx, item) in refs.iter().enumerate() {
        images.push(
            preprocessor
                .prepare(
                    &format!("character_references[{idx}].image"),
                    &item.image,
                    ImagePreparation::CharacterReference,
                )?
                .to_base64(),
        );
        descriptions.push(ReferenceConditionInput {
            caption: Some(ReferenceCaption {
                base_caption: item.reference_type.as_str().to_owned(),
                char_captions: Vec::new(),
            }),
            legacy_uc: Some(false),
        });
        information_extracted.push(1.0);
        secondary_strength_values.push(1.0 - item.fidelity);
        strength_values.push(item.strength);
    }

    Ok(CharacterReferenceWireParts {
        images,
        descriptions,
        information_extracted,
        secondary_strength_values,
        strength_values,
    })
}
