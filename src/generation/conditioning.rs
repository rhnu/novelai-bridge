use serde::{Deserialize, Serialize};

use super::{CharacterPosition, GenerateImageRequest, GenerationCore};

const TRANSPARENT_BACKGROUND_TAG: &str = ", transparent background";
const FURRY_DATASET_TAG: &str = "fur dataset";

/// Final conditioning text a request will actually send, per field.
///
/// A prompt is contributed to by several sources: the caller's text, the furry-dataset prefix,
/// the transparent-background tag, [`crate::QualityPreset`] tags and the [`crate::UcPreset`] text. Anything
/// that inspects prompt text —
/// most importantly token counting — must look at the assembled result rather than at
/// [`GenerationCore::prompt`], or it will under-report. Request building and token counting both
/// read this one type, so the counted text cannot drift from the sent text.
///
/// Obtain a value through [`GenerateImageRequest::resolve_prompts`].
#[non_exhaustive]
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedPrompts {
    /// Positive prompt including the furry prefix, transparency tag and quality tags.
    pub prompt: String,
    /// Negative prompt including the UC preset text.
    pub negative_prompt: String,
    /// Character prompts in request order, empty when the model has no character support.
    ///
    /// Disabled characters are omitted; original indices are preserved.
    pub characters: Vec<ResolvedCharacterPrompt>,
    /// Effective character positioning flag.
    pub use_coords: bool,
}

/// Final conditioning text for one character prompt.
///
/// Character prompts receive neither quality tags nor UC preset text; they are sent as written.
#[non_exhaustive]
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedCharacterPrompt {
    /// Index in the originating [`crate::CharacterSet::characters`].
    pub index: usize,
    /// Effective normalized position used by every wire representation.
    pub position: CharacterPosition,
    /// Positive character prompt.
    pub prompt: String,
    /// Negative character prompt, empty when unset.
    pub negative_prompt: String,
}

impl std::fmt::Debug for ResolvedPrompts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedPrompts")
            .field("characters", &self.characters.len())
            .field("use_coords", &self.use_coords)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for ResolvedCharacterPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedCharacterPrompt")
            .field("index", &self.index)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}

impl GenerationCore {
    /// Assembles the positive prompt from every contributing source.
    fn assemble_prompt(&self) -> String {
        let mut prompt = if self.furry_mode {
            if self.prompt.is_empty() {
                FURRY_DATASET_TAG.to_owned()
            } else {
                format!("{FURRY_DATASET_TAG}, {}", self.prompt)
            }
        } else {
            self.prompt.clone()
        };
        if self.transparent_background {
            prompt.push_str(TRANSPARENT_BACKGROUND_TAG);
        }
        prompt.push_str(self.quality.tags());
        prompt
    }

    /// Assembles the negative prompt from every contributing source.
    ///
    /// The UC preset text is placed before the caller's negative prompt, matching the `NovelAI`
    /// web client.
    fn assemble_negative_prompt(&self) -> String {
        let preset = self.uc_preset.preset_text();
        let negative = self.negative_prompt.as_deref().unwrap_or_default();
        match (preset.is_empty(), negative.is_empty()) {
            (true, _) => negative.to_owned(),
            (false, true) => preset.to_owned(),
            (false, false) => format!("{preset}, {negative}"),
        }
    }
}

#[cfg(test)]
mod tests;

impl GenerateImageRequest {
    /// Assembles already-normalized input; callers must use the generation boundary.
    pub(crate) fn assemble_prompts(&self) -> ResolvedPrompts {
        let characters = self
            .characters
            .as_ref()
            .map(|set| {
                set.characters
                    .iter()
                    .enumerate()
                    .filter(|(_, character)| character.enabled)
                    .map(|(index, character)| ResolvedCharacterPrompt {
                        index,
                        position: character.position,
                        prompt: character.prompt.clone(),
                        negative_prompt: character.negative_prompt.clone().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        ResolvedPrompts {
            prompt: self.core.assemble_prompt(),
            negative_prompt: self.core.assemble_negative_prompt(),
            characters,
            use_coords: self
                .characters
                .as_ref()
                .and_then(|set| set.use_coords)
                .unwrap_or(false),
        }
    }
}
