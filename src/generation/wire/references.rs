use serde::{Serialize, Serializer, ser::SerializeStruct};

use super::{ReferenceCaption, ReferenceConditionInput};
use crate::{
    BridgeError, CharacterReferenceField, GenerationField, ImagePreparation, ImagePreprocessor,
    ReferenceSet,
};

pub(super) struct VibeRecord {
    image: String,
    strength: f32,
}

pub(super) struct CharacterReferenceRecord {
    image: String,
    description: ReferenceConditionInput,
    secondary_strength: f32,
    strength: f32,
}

/// References retain row identity until the final provider-specific parallel-array projection.
/// Normalization has already applied character/vibe precedence.
pub(super) enum ReferenceParameters {
    None,
    Vibe {
        references: Vec<VibeRecord>,
        strength: f32,
    },
    Character(Vec<CharacterReferenceRecord>),
}

pub(super) fn prepare_references(
    references: Option<&ReferenceSet>,
    preprocessor: Option<&dyn ImagePreprocessor>,
) -> Result<ReferenceParameters, BridgeError> {
    let Some(references) = references else {
        return Ok(ReferenceParameters::None);
    };
    if !references.character_references.is_empty() {
        let records = references
            .character_references
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let image = crate::image::prepare_image(
                    preprocessor,
                    GenerationField::CharacterReference {
                        index,
                        field: CharacterReferenceField::Image,
                    }
                    .into(),
                    &item.image,
                    ImagePreparation::character_reference(item.image.size()),
                )?
                .to_base64();
                Ok(CharacterReferenceRecord {
                    image,
                    description: ReferenceConditionInput {
                        caption: ReferenceCaption {
                            base_caption: item.reference_type.as_str(),
                            char_captions: Vec::new(),
                        },
                        legacy_uc: false,
                    },
                    secondary_strength: 1.0 - item.fidelity,
                    strength: item.strength,
                })
            })
            .collect::<Result<Vec<_>, BridgeError>>()?;
        return Ok(ReferenceParameters::Character(records));
    }
    Ok(references
        .vibe_transfer
        .as_ref()
        .map_or(ReferenceParameters::None, |vibe| {
            ReferenceParameters::Vibe {
                references: vibe
                    .references
                    .iter()
                    .map(|reference| VibeRecord {
                        image: reference.vibe_data_cache.to_base64(),
                        strength: reference.strength,
                    })
                    .collect(),
                strength: vibe.strength,
            }
        }))
}

impl Serialize for ReferenceParameters {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => {
                let mut fields = serializer.serialize_struct("References", 1)?;
                fields.serialize_field("controlnet_strength", &1f32)?;
                fields.end()
            },
            Self::Vibe {
                references,
                strength,
            } => {
                let mut fields = serializer.serialize_struct("References", 4)?;
                fields.serialize_field(
                    "reference_image_multiple",
                    &references
                        .iter()
                        .map(|item| &item.image)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field(
                    "reference_strength_multiple",
                    &references
                        .iter()
                        .map(|item| item.strength)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field("normalize_reference_strength_multiple", &true)?;
                fields.serialize_field("controlnet_strength", strength)?;
                fields.end()
            },
            Self::Character(references) => {
                let mut fields = serializer.serialize_struct("References", 6)?;
                fields.serialize_field(
                    "director_reference_images",
                    &references
                        .iter()
                        .map(|item| &item.image)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field(
                    "director_reference_descriptions",
                    &references
                        .iter()
                        .map(|item| &item.description)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field(
                    "director_reference_information_extracted",
                    &vec![1f32; references.len()],
                )?;
                fields.serialize_field(
                    "director_reference_secondary_strength_values",
                    &references
                        .iter()
                        .map(|item| item.secondary_strength)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field(
                    "director_reference_strength_values",
                    &references
                        .iter()
                        .map(|item| item.strength)
                        .collect::<Vec<_>>(),
                )?;
                fields.serialize_field("controlnet_strength", &1f32)?;
                fields.end()
            },
        }
    }
}
