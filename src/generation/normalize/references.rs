use super::{Field, Normalizer};
use crate::{
    BridgeError, GenerateImageRequest, InvalidRequest, ModelDescriptor, ReferenceAvailability,
    ReferenceIntent, ReferenceSet, VibeTransferConfig,
    generation::{CharacterReferenceField, ImageInputField, NormalizationChange},
};

impl Normalizer {
    pub(super) fn normalize_references(
        &mut self,
        params: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        let mode = params.mode();
        let Some(references) = params.references.as_mut() else {
            return Ok(());
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
        let resolution = descriptor.resolve_references(mode, intent);
        if intent.character
            && !self.reference_allowed(Field::CharacterReferences, resolution.character)?
        {
            references.character_references.clear();
        }
        if intent.vibe && !self.reference_allowed(Field::VibeTransfer, resolution.vibe)? {
            references.vibe_transfer = None;
        }
        if references.is_empty() {
            params.references = None;
        }
        Ok(())
    }

    pub(super) fn reference_allowed(
        &mut self,
        field: Field,
        availability: ReferenceAvailability,
    ) -> Result<bool, BridgeError> {
        match availability {
            ReferenceAvailability::Available => Ok(true),
            ReferenceAvailability::NotRequested => Ok(false),
            ReferenceAvailability::Unsupported | ReferenceAvailability::InputNotImplemented => {
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
        if self.reject {
            return Err(InvalidRequest::UnsupportedFieldCombination {
                left: Field::VibeTransfer.to_string(),
                right: retained_field.to_string(),
            }
            .into());
        }
        self.record(
            Field::VibeTransfer,
            NormalizationChange::Conflict { retained_field },
        );
        Ok(false)
    }

    pub(super) fn normalize_character_references(
        &mut self,
        references: &mut ReferenceSet,
    ) -> Result<(), BridgeError> {
        for (idx, item) in references.character_references.iter_mut().enumerate() {
            item.fidelity = self.normalize_f32_range(
                Field::CharacterReference {
                    index: idx,
                    field: CharacterReferenceField::Fidelity,
                },
                item.fidelity,
                0.0,
                1.0,
            )?;
            item.strength = self.normalize_f32_range(
                Field::CharacterReference {
                    index: idx,
                    field: CharacterReferenceField::Strength,
                },
                item.strength,
                0.0,
                1.0,
            )?;
        }

        Ok(())
    }

    pub(super) fn normalize_vibe_transfer(
        &mut self,
        vibe: Option<&mut VibeTransferConfig>,
    ) -> Result<(), BridgeError> {
        let Some(vibe) = vibe else {
            return Ok(());
        };

        vibe.strength = self.normalize_f32_range(Field::VibeStrength, vibe.strength, 0.0, 1.0)?;
        for (idx, reference) in vibe.references.iter_mut().enumerate() {
            reference.strength = self.normalize_f32_range(
                Field::VibeReferenceStrength(idx),
                reference.strength,
                0.0,
                1.0,
            )?;
        }

        Ok(())
    }
}
