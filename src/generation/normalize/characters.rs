use super::{Field, Normalizer};
use crate::{
    BridgeError, CharacterField, CharacterPositionProfile, CharacterSet, GenerateImageRequest,
    InvalidRequest, ModelDescriptor,
    generation::{NormalizationChange, characters::is_center_position},
};

impl Normalizer {
    pub(super) fn gate_characters(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if descriptor.prompt().characters.is_none()
            && params
                .characters
                .as_ref()
                .is_some_and(|set| set.enabled().next().is_some())
            && !self.require_capability(
                false,
                Field::Characters,
                "a model supporting structured character prompts",
                NormalizationChange::Unsupported,
            )?
        {
            params.characters = None;
        }

        if let Some(set) = params.characters.as_mut() {
            let limit = descriptor.prompt().max_characters() as usize;
            if self.policy.is_reject() {
                let enabled_count = set.enabled_count();
                if enabled_count > limit {
                    return Err(InvalidRequest::NumericOutOfRange {
                        field: Field::Characters.into(),
                        value: f64::from(u32::try_from(enabled_count).unwrap_or(u32::MAX)),
                        min: 0.0,
                        max: f64::from(descriptor.prompt().max_characters()),
                    }
                    .into());
                }
                return Ok(());
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
        let use_coords = resolve_use_coords(set);
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

fn resolve_use_coords(characters: &CharacterSet) -> bool {
    match characters.use_coords {
        Some(false) => false,
        Some(true) => characters.enabled().next().is_some(),
        None => characters.enabled().any(|character| {
            !is_center_position(character.position.x) || !is_center_position(character.position.y)
        }),
    }
}
