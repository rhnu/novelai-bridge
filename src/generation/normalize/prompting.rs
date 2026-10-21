use super::{Field, Normalizer};
use crate::{BridgeError, CharacterField, GenerateImageRequest, ModelDescriptor};

impl Normalizer {
    pub(super) fn normalize_custom_negative_prompts(
        &mut self,
        request: &mut GenerateImageRequest,
        descriptor: &ModelDescriptor,
    ) -> Result<(), BridgeError> {
        if descriptor.prompt().custom_negative_prompts {
            return Ok(());
        }
        if request
            .core
            .negative_prompt
            .as_ref()
            .is_some_and(|text| !text.is_empty())
        {
            self.require_capability(
                false,
                Field::NegativePrompt,
                "a model supporting custom negative prompts",
                super::NormalizationChange::Unsupported,
            )?;
        }
        request.core.negative_prompt = None;
        if let Some(characters) = request.characters.as_mut() {
            for (index, character) in characters
                .characters
                .iter_mut()
                .enumerate()
                .filter(|(_, value)| value.enabled)
            {
                if character
                    .negative_prompt
                    .as_ref()
                    .is_some_and(|text| !text.is_empty())
                {
                    self.require_capability(
                        false,
                        Field::Character {
                            index,
                            field: CharacterField::NegativePrompt,
                        },
                        "a model supporting custom character negative prompts",
                        super::NormalizationChange::Unsupported,
                    )?;
                }
                character.negative_prompt = None;
            }
        }
        Ok(())
    }
}
