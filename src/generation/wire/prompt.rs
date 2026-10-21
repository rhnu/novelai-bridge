use super::{CenterPoint, CharacterPrompt, V4CharacterCaption};
use crate::ResolvedCharacterPrompt;

const fn center_point(character: &ResolvedCharacterPrompt) -> CenterPoint {
    CenterPoint {
        x: character.position.x,
        y: character.position.y,
    }
}

pub(super) fn build_character_captions(
    resolved: &[ResolvedCharacterPrompt],
    negative: bool,
) -> Vec<V4CharacterCaption> {
    resolved
        .iter()
        .map(|character| V4CharacterCaption {
            centers: vec![center_point(character)],
            char_caption: if negative {
                character.negative_prompt.clone()
            } else {
                character.prompt.clone()
            },
        })
        .collect()
}

pub(super) fn build_character_prompts(
    resolved: &[ResolvedCharacterPrompt],
) -> Vec<CharacterPrompt> {
    resolved
        .iter()
        .map(|character| CharacterPrompt {
            prompt: character.prompt.clone(),
            uc: character.negative_prompt.clone(),
            center: center_point(character),
            enabled: true,
        })
        .collect()
}
