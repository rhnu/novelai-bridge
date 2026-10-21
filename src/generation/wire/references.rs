use crate::{BridgeError, CharacterReference, ImagePreparation, ImagePreprocessor};
use super::{ReferenceCaption, ReferenceConditionInput};

pub(crate) struct CharacterReferenceWireParts {
    pub images: Vec<String>,
    pub descriptions: Vec<ReferenceConditionInput>,
    pub information_extracted: Vec<f32>,
    pub secondary_strength_values: Vec<f32>,
    pub strength_values: Vec<f32>,
}

pub(crate) fn convert_character_references(
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
            crate::image::prepare_image(
                    preprocessor,
                    &format!("references.character_references[{idx}].image"),
                    &item.image,
                    ImagePreparation::character_reference(item.image.size()),
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
