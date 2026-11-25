use super::{Field, Normalizer};
use crate::{
    BridgeError, CharacterReferenceField, GenerateImageRequest, ImageInputField, InvalidRequest,
    ModelDescriptor, ReferenceAvailability, ReferenceIntent, ReferenceProfile, ReferenceSet,
    VibeInputPath, VibeReferenceField, VibeReferenceInput, VibeTransferConfig,
    generation::NormalizationChange,
};

/// Original positions of the effective reference groups.
#[derive(Default)]
pub(super) struct ReferenceSourceIndices {
    pub(super) character: Vec<usize>,
    pub(super) vibe: Vec<usize>,
}

impl Normalizer {
    pub(super) fn normalize_references(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<ReferenceSourceIndices, BridgeError> {
        let mut indices = ReferenceSourceIndices::default();
        let mode = params.mode();
        let Some(references) = params.references.as_mut() else {
            return Ok(indices);
        };
        if references
            .vibe_transfer
            .as_ref()
            .is_some_and(|vibe| vibe.references.is_empty())
        {
            references.vibe_transfer = None;
        }
        let intent = ReferenceIntent {
            character: !references.character_references.is_empty(),
            vibe: references
                .vibe_transfer
                .as_ref()
                .is_some_and(|vibe| !vibe.references.is_empty()),
        };
        let mut resolution = descriptor.resolve_references(mode, intent);
        if intent.character {
            if self.reference_allowed(Field::CharacterReferences, resolution.character)? {
                let profile = descriptor
                    .support()
                    .character_reference
                    .expect("normalization uses the effective reference model");
                indices.character = self.normalize_character_references(references, profile)?;
            } else {
                references.character_references.clear();
            }
            if indices.character.is_empty()
                && resolution.vibe == ReferenceAvailability::ConflictWithCharacterReference
            {
                resolution = descriptor.resolve_references(
                    mode,
                    ReferenceIntent {
                        character: false,
                        ..intent
                    },
                );
            }
        }
        if intent.vibe && !self.reference_allowed(Field::VibeTransfer, resolution.vibe)? {
            references.vibe_transfer = None;
        }
        if let Some(vibe) = references.vibe_transfer.as_mut() {
            indices.vibe = self.normalize_vibe_transfer(vibe, descriptor)?;
            if vibe.references.is_empty() {
                references.vibe_transfer = None;
            }
        }
        if references.is_empty() {
            params.references = None;
        }
        Ok(indices)
    }

    pub(super) fn reference_allowed(
        &mut self,
        field: Field,
        availability: ReferenceAvailability,
    ) -> Result<bool, BridgeError> {
        match availability {
            ReferenceAvailability::Available => Ok(true),
            ReferenceAvailability::NotRequested => Ok(false),
            ReferenceAvailability::Unsupported => {
                let required = "a model implementing this reference capability";
                self.require_capability(false, field, required, NormalizationChange::Unsupported)
            },
            ReferenceAvailability::UnsupportedMode if field == Field::VibeTransfer => {
                self.reference_conflict(Field::ImageInput(ImageInputField::Mask))
            },
            ReferenceAvailability::UnsupportedMode => self.require_capability(
                false,
                Field::CharacterReferences,
                "models with character-reference inpainting",
                NormalizationChange::Unsupported,
            ),
            ReferenceAvailability::ConflictWithCharacterReference => {
                self.reference_conflict(Field::CharacterReferences)
            },
        }
    }

    pub(super) fn reference_conflict(
        &mut self,
        retained_field: Field,
    ) -> Result<bool, BridgeError> {
        if self.policy.is_reject() {
            return Err(InvalidRequest::UnsupportedFieldCombination {
                left: Field::VibeTransfer.into(),
                right: retained_field.into(),
            }
            .into());
        }
        self.record(
            Field::VibeTransfer,
            NormalizationChange::Conflict { retained_field },
        );
        Ok(false)
    }

    fn normalize_character_references(
        &mut self,
        references: &mut ReferenceSet,
        profile: ReferenceProfile,
    ) -> Result<Vec<usize>, BridgeError> {
        let capacity = references
            .character_references
            .len()
            .min(profile.max_references);
        let mut retained = Vec::with_capacity(capacity);
        let mut indices = Vec::with_capacity(capacity);
        for (index, mut item) in std::mem::take(&mut references.character_references)
            .into_iter()
            .enumerate()
        {
            let field = |field| Field::CharacterReference { index, field };
            let type_field = field(CharacterReferenceField::ReferenceType);
            let Some(effective_type) = profile.resolve_type(item.reference_type) else {
                self.require_capability(
                    false,
                    type_field,
                    "a model supporting this reference type",
                    NormalizationChange::Unsupported,
                )?;
                continue;
            };
            self.require_capability(
                effective_type == item.reference_type,
                type_field,
                "a model supporting this outfit preference",
                NormalizationChange::Fallback,
            )?;
            item.reference_type = effective_type;

            if retained.len() == profile.max_references {
                let field = field(CharacterReferenceField::Entry);
                let limit = profile.max_references;
                if self.policy.is_reject() {
                    return Err(InvalidRequest::TooManyCharacterReferences {
                        field: field.into(),
                        limit,
                    }
                    .into());
                }
                self.record(field, NormalizationChange::ReferenceLimit { limit });
                continue;
            }

            let fidelity_field = field(CharacterReferenceField::Fidelity);
            if profile.fidelity {
                Self::validate_reference_number(fidelity_field, item.fidelity)?;
                Self::validate_reference_number(fidelity_field, 1.0 - item.fidelity)?;
            } else {
                item.fidelity =
                    self.normalize_f32_range(fidelity_field, item.fidelity, 1.0, 1.0)?;
            }
            Self::validate_reference_number(
                field(CharacterReferenceField::Strength),
                item.strength,
            )?;
            item.information_extracted = self.normalize_f32_range(
                field(CharacterReferenceField::InformationExtracted),
                item.information_extracted,
                0.0,
                1.0,
            )?;
            retained.push(item);
            indices.push(index);
        }
        references.character_references = retained;
        Ok(indices)
    }

    fn normalize_vibe_transfer(
        &mut self,
        vibe: &mut VibeTransferConfig,
        descriptor: &ModelDescriptor,
    ) -> Result<Vec<usize>, BridgeError> {
        let path = descriptor
            .support()
            .vibe
            .expect("vibe capability has been resolved")
            .input;
        let mut retained = Vec::with_capacity(vibe.references.len());
        let mut indices = Vec::with_capacity(vibe.references.len());
        for (index, mut reference) in std::mem::take(&mut vibe.references).into_iter().enumerate() {
            let matches_path = matches!(
                (&reference.input, path),
                (VibeReferenceInput::Encoded { .. }, VibeInputPath::Encoded)
                    | (VibeReferenceInput::Image { .. }, VibeInputPath::RawImage)
            );
            if !self.require_capability(
                matches_path,
                Field::VibeReference {
                    index,
                    field: VibeReferenceField::Input,
                },
                match path {
                    VibeInputPath::Encoded => "a model accepting encoded vibe references",
                    VibeInputPath::RawImage => "a model accepting raw image vibe references",
                },
                NormalizationChange::Unsupported,
            )? {
                continue;
            }
            Self::validate_reference_number(
                Field::VibeReference {
                    index,
                    field: VibeReferenceField::Strength,
                },
                reference.strength,
            )?;
            if let VibeReferenceInput::Image {
                information_extracted,
                ..
            } = &mut reference.input
            {
                *information_extracted = self.normalize_f32_range(
                    Field::VibeReference {
                        index,
                        field: VibeReferenceField::InformationExtracted,
                    },
                    *information_extracted,
                    0.0,
                    1.0,
                )?;
            }
            retained.push(reference);
            indices.push(index);
        }
        if matches!(path, VibeInputPath::Encoded) && vibe.normalize_strength && retained.len() > 1 {
            // Project once for this plan. Editing its effective f32 weights and normalizing
            // again creates a new plan; it can incur another floating point rounding step.
            let absolute_sum: f64 = retained
                .iter()
                .map(|reference| f64::from(reference.strength).abs())
                .sum();
            if absolute_sum > 1.0 {
                for (reference, &index) in retained.iter_mut().zip(&indices) {
                    let original = reference.strength;
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "Normalized strength is bounded by one and represented by the API's f32 field"
                    )]
                    let effective = (f64::from(original) / absolute_sum) as f32;
                    reference.strength = effective;
                    self.adjusted(
                        Field::VibeReference {
                            index,
                            field: VibeReferenceField::Strength,
                        },
                        f64::from(original),
                        f64::from(effective),
                    );
                }
            }
        }
        vibe.references = retained;
        Ok(indices)
    }

    fn validate_reference_number(field: Field, value: f32) -> Result<(), BridgeError> {
        if value.is_finite() {
            Ok(())
        } else {
            Err(InvalidRequest::NonFiniteNumber {
                field: field.into(),
                value: f64::from(value),
            }
            .into())
        }
    }
}

#[cfg(test)]
mod tests;
