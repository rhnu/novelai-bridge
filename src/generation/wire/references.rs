use std::{collections::HashMap, mem::Discriminant};

use serde::{Serialize, Serializer, ser::SerializeStruct};

use super::{ReferenceCaption, ReferenceConditionInput};
use crate::{
    BridgeError, CharacterReferenceField, GenerationField, ImagePreparation, ImagePreprocessor,
    ReferenceSet, VibeReferenceField, VibeReferenceInput,
    image::request_body::{Media, MediaSlot},
};

pub(super) struct VibeRecord {
    image: Media,
    strength: f32,
    information_extracted: Option<f32>,
}

pub(super) struct CharacterReferenceRecord {
    image: Media,
    description: ReferenceConditionInput,
    secondary_strength: f32,
    strength: f32,
    information_extracted: f32,
}

/// References retain row identity until the final provider-specific parallel-array projection.
/// Normalization has already applied character/vibe precedence.
pub(super) enum ReferenceParameters {
    None,
    Vibe {
        references: Vec<VibeRecord>,
        normalize_strength: Option<bool>,
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
    let mut images = ReferenceImages {
        preprocessor,
        prepared: HashMap::new(),
    };
    if !references.character_references.is_empty() {
        let records = references
            .character_references
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let image = Media::Prepared(
                    images.prepare(
                        GenerationField::CharacterReference {
                            index,
                            field: CharacterReferenceField::Image,
                        }
                        .into(),
                        &item.image,
                        ImagePreparation::character_reference(item.image.size()),
                    )?,
                );
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
                    information_extracted: item.information_extracted,
                })
            })
            .collect::<Result<Vec<_>, BridgeError>>()?;
        return Ok(ReferenceParameters::Character(records));
    }
    let Some(vibe) = references.vibe_transfer.as_ref() else {
        return Ok(ReferenceParameters::None);
    };
    let records = vibe
        .references
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            let (image, information_extracted) = match &reference.input {
                VibeReferenceInput::Encoded { encoding } => (Media::Vibe(encoding.clone()), None),
                VibeReferenceInput::Image {
                    image,
                    information_extracted,
                } => (
                    Media::Prepared(
                        images.prepare(
                            GenerationField::VibeReference {
                                index,
                                field: VibeReferenceField::Image,
                            }
                            .into(),
                            image,
                            ImagePreparation::RawVibe,
                        )?,
                    ),
                    Some(*information_extracted),
                ),
            };
            Ok(VibeRecord {
                image,
                strength: reference.strength,
                information_extracted,
            })
        })
        .collect::<Result<Vec<_>, BridgeError>>()?;
    let encoded = records
        .first()
        .is_some_and(|record| record.information_extracted.is_none());
    Ok(ReferenceParameters::Vibe {
        references: records,
        normalize_strength: encoded.then_some(vibe.normalize_strength),
    })
}

#[derive(Hash, PartialEq, Eq)]
struct PreparationKey {
    pixels: *const image::DynamicImage,
    operation: Discriminant<ImagePreparation>,
    width: u32,
    height: u32,
}

/// Shares deterministic built-in work within one request without merging reference rows.
struct ReferenceImages<'a> {
    preprocessor: Option<&'a dyn ImagePreprocessor>,
    prepared: HashMap<PreparationKey, crate::PreparedImage>,
}

impl ReferenceImages<'_> {
    fn prepare(
        &mut self,
        field: crate::RequestField,
        image: &crate::EncodedImage,
        operation: ImagePreparation,
    ) -> Result<crate::PreparedImage, BridgeError> {
        // Custom preprocessors may use the row's field/index to produce distinct images.
        if self.preprocessor.is_some() {
            return crate::image::prepare_image(self.preprocessor, field, image, operation);
        }
        let size = operation.target_size();
        let key = PreparationKey {
            pixels: std::ptr::from_ref(image.pixels()),
            operation: std::mem::discriminant(&operation),
            width: size.width,
            height: size.height,
        };
        match self.prepared.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => Ok(entry.get().clone()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let prepared = crate::image::prepare_image(None, field, image, operation)?;
                entry.insert(prepared.clone());
                Ok(prepared)
            },
        }
    }
}

impl ReferenceParameters {
    pub(super) fn extract_media(&mut self, media: &mut Vec<MediaSlot>) {
        match self {
            Self::None => {},
            Self::Vibe { references, .. } => {
                for (index, reference) in references.iter_mut().enumerate() {
                    reference.image.extract(
                        format!("/parameters/reference_image_multiple/{index}"),
                        media,
                    );
                }
            },
            Self::Character(references) => {
                for (index, reference) in references.iter_mut().enumerate() {
                    reference.image.extract(
                        format!("/parameters/director_reference_images/{index}"),
                        media,
                    );
                }
            },
        }
    }
}

impl Serialize for ReferenceParameters {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::None => {
                let fields = serializer.serialize_struct("References", 0)?;
                fields.end()
            },
            Self::Vibe {
                references,
                normalize_strength,
            } => {
                let mut fields = serializer.serialize_struct("References", 3)?;
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
                if let Some(normalize) = normalize_strength {
                    fields.serialize_field("normalize_reference_strength_multiple", normalize)?;
                } else {
                    fields.serialize_field(
                        "reference_information_extracted_multiple",
                        &references
                            .iter()
                            .map(|item| {
                                item.information_extracted.expect(
                                    "normalized raw image references retain information extraction",
                                )
                            })
                            .collect::<Vec<_>>(),
                    )?;
                }
                fields.end()
            },
            Self::Character(references) => {
                let mut fields = serializer.serialize_struct("References", 5)?;
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
                    &references
                        .iter()
                        .map(|item| item.information_extracted)
                        .collect::<Vec<_>>(),
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
                fields.end()
            },
        }
    }
}

#[cfg(test)]
mod tests;
