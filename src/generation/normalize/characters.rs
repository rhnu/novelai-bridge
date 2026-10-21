use super::{Field, Normalizer};
use crate::{
    BridgeError, Character, CharacterField, CharacterPositionProfile, CharacterSet,
    GenerateImageRequest, InvalidRequest, ModelDescriptor,
    generation::{NormalizationChange, characters::is_center_position},
};

impl Normalizer {
    pub(super) fn gate_characters(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if params
            .characters
            .as_ref()
            .is_some_and(|set| set.enabled_count() > 0)
            && !self.require_capability(
                descriptor.prompt().characters.is_some(),
                Field::Characters,
                "a model supporting structured character prompts",
                NormalizationChange::Unsupported,
            )?
        {
            params.characters = None;
        }

        if let Some(set) = params.characters.as_mut() {
            let limit = descriptor.prompt().max_characters() as usize;
            if set.enabled_count() > limit && self.policy.is_reject() {
                return Err(InvalidRequest::NumericOutOfRange {
                    field: Field::Characters.into(),
                    value: f64::from(u32::try_from(set.enabled_count()).unwrap_or(u32::MAX)),
                    min: 0.0,
                    max: f64::from(descriptor.prompt().max_characters()),
                }
                .into());
            }
            for (index, character) in set
                .characters
                .iter_mut()
                .enumerate()
                .filter(|(_, character)| character.enabled)
                .skip(limit)
            {
                character.enabled = false;
                self.record(
                    Field::Character {
                        index,
                        field: CharacterField::Entry,
                    },
                    NormalizationChange::CharacterLimit {
                        index,
                        limit: descriptor.prompt().max_characters(),
                    },
                );
            }
        }
        Ok(())
    }

    pub(super) fn validate_character_inputs(characters: &[Character]) -> Result<(), BridgeError> {
        for (index, character) in characters
            .iter()
            .enumerate()
            .filter(|(_, character)| character.enabled)
        {
            if character.prompt.trim().is_empty() {
                return Err(InvalidRequest::EmptyField {
                    field: Field::Character {
                        index,
                        field: CharacterField::Prompt,
                    }
                    .into(),
                }
                .into());
            }
        }
        Ok(())
    }

    pub(super) fn normalize_character_positions(
        &mut self,
        set: &mut CharacterSet,
        positioning: CharacterPositionProfile,
    ) -> Result<(), BridgeError> {
        for (index, ch) in set
            .characters
            .iter_mut()
            .enumerate()
            .filter(|(_, ch)| ch.enabled)
        {
            ch.position.x = self.normalize_f32_range(
                Field::Character {
                    index,
                    field: CharacterField::X,
                },
                ch.position.x,
                0.0,
                1.0,
            )?;
            ch.position.y = self.normalize_f32_range(
                Field::Character {
                    index,
                    field: CharacterField::Y,
                },
                ch.position.y,
                0.0,
                1.0,
            )?;
        }
        // Preserve automatic placement semantics before applying model-specific geometry.
        let use_coords = resolve_use_coords(Some(set));
        set.use_coords = Some(use_coords);
        for (index, ch) in set
            .characters
            .iter_mut()
            .enumerate()
            .filter(|(_, ch)| ch.enabled)
        {
            for (axis, value) in [
                (CharacterField::X, &mut ch.position.x),
                (CharacterField::Y, &mut ch.position.y),
            ] {
                let effective = if use_coords {
                    positioning.resolve_coordinate(*value)
                } else {
                    0.5
                };
                self.adjusted(
                    Field::Character { index, field: axis },
                    f64::from(*value),
                    f64::from(effective),
                );
                *value = effective;
            }
        }
        Ok(())
    }
}

fn resolve_use_coords(characters: Option<&CharacterSet>) -> bool {
    let Some(characters) = characters else {
        return false;
    };
    if characters.enabled_count() == 0 {
        return false;
    }
    if let Some(explicit) = characters.use_coords {
        return explicit;
    }
    characters.enabled().any(|character| {
        !is_center_position(character.position.x) || !is_center_position(character.position.y)
    })
}
