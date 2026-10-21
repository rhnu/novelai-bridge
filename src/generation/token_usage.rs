use crate::{GenerateImageRequest, GenerateImageStreamRequest, ImageTokenizerKind, FramingPolicy, TokenCount, TokenizerError, Tokenizers};

/// Identifies which prompt field a token count belongs to.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PromptField {
    /// The positive prompt, including quality and transparency tags.
    Prompt,
    /// The negative prompt, including the UC preset text.
    NegativePrompt,
    /// One character prompt.
    Character {
        /// Index in the request's character set.
        index: usize,
        /// Whether this is the character's negative prompt.
        negative: bool,
    },
}

/// Token usage of one character prompt.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterTokenUsage {
    /// Index in the request's character set.
    pub index: usize,
    /// Positive character prompt usage.
    pub prompt: TokenCount,
    /// Negative character prompt usage.
    pub negative_prompt: TokenCount,
}

/// Per-field token usage for a generation request.
///
/// Counts are deliberately **not** summed. `prompt_token_limit` bounds one conditioning text, and
/// there is no wire basis for adding the positive, negative and character prompts together. A
/// caller that wants an aggregate for its own UI can compute one from these fields.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptTokenUsage {
    /// Encoder the model's descriptor selected.
    pub tokenizer: ImageTokenizerKind,
    /// The model's prompt token limit, repeated on every [`TokenCount`] for convenience.
    pub limit: u32,
    /// Positive prompt usage.
    pub prompt: TokenCount,
    /// Negative prompt usage.
    pub negative_prompt: TokenCount,
    /// Character prompt usage in request order.
    pub characters: Vec<CharacterTokenUsage>,
}

impl PromptTokenUsage {
    /// Returns every field that exceeds the model's limit.
    ///
    /// Only enabled characters appear in resolved prompts and usage.
    pub fn over_limit(&self) -> impl Iterator<Item = (PromptField, TokenCount)> + '_ {
        let base = [
            (PromptField::Prompt, self.prompt),
            (PromptField::NegativePrompt, self.negative_prompt),
        ];
        let characters = self.characters.iter().flat_map(|usage| {
            [
                (
                    PromptField::Character {
                        index: usage.index,
                        negative: false,
                    },
                    usage.prompt,
                ),
                (
                    PromptField::Character {
                        index: usage.index,
                        negative: true,
                    },
                    usage.negative_prompt,
                ),
            ]
        });

        base.into_iter()
            .chain(characters)
            .filter(|(_, count)| count.is_over_limit())
    }

    /// Returns `true` when any counted field exceeds the model's limit.
    #[must_use]
    pub fn is_over_limit(&self) -> bool {
        self.over_limit().next().is_some()
    }
}

impl PromptField {
    /// Returns a human-readable field name for diagnostics.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Prompt => "prompt".to_owned(),
            Self::NegativePrompt => "negative_prompt".to_owned(),
            Self::Character { index, negative } => {
                let suffix = if *negative {
                    ".negative_prompt"
                } else {
                    ".prompt"
                };
                format!("characters[{index}]{suffix}")
            }
        }
    }
}

impl GenerateImageRequest {
    /// Normalizes and counts effective prompts using model-input framing.
    ///
    /// # Errors
    ///
    /// Returns normalization or tokenizer errors.
    pub fn count_tokens(
        &self,
        tokenizers: &Tokenizers,
    ) -> Result<PromptTokenUsage, crate::BridgeError> {
        self.count_tokens_with(tokenizers, FramingPolicy::ModelInput)
    }

    /// Normalizes and counts effective prompts using explicit framing.
    ///
    /// # Errors
    ///
    /// Returns normalization or tokenizer errors.
    pub fn count_tokens_with(
        &self,
        tokenizers: &Tokenizers,
        framing: FramingPolicy,
    ) -> Result<PromptTokenUsage, crate::BridgeError> {
        Ok(self
            .clone()
            .normalize()?
            .count_tokens_with(tokenizers, framing)?)
    }
}

impl crate::NormalizedGeneration {
    /// Counts the tokens of every prompt field this request will send.
    ///
    /// Counting uses [`FramingPolicy::ModelInput`], matching what the model consumes and therefore
    /// what the prompt limit is measured against. The counted text comes from
    /// [`crate::NormalizedGeneration::resolved_prompts`], so quality tags, the transparency tag and the UC
    /// preset are all included; counting the raw prompt fields instead would under-report.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError::TokenizerNotLoaded`] when the registry has no encoder for the
    /// selected model.
    pub fn count_tokens(
        &self,
        tokenizers: &Tokenizers,
    ) -> Result<PromptTokenUsage, TokenizerError> {
        self.count_tokens_with(tokenizers, FramingPolicy::ModelInput)
    }

    /// Counts prompt tokens under an explicit framing policy.
    ///
    /// A prompt editor that wants to show content length alone can pass
    /// [`FramingPolicy::ContentOnly`].
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError::TokenizerNotLoaded`] when the registry has no encoder for the
    /// selected model.
    pub fn count_tokens_with(
        &self,
        tokenizers: &Tokenizers,
        framing: FramingPolicy,
    ) -> Result<PromptTokenUsage, TokenizerError> {
        let descriptor = self.request().descriptor();
        let encoder = tokenizers.require_for_model(self.request().core.model)?;
        let resolved = self.resolved_prompts();
        let limit = descriptor.prompt().token_limit;
        let count = |text: &str| -> Result<TokenCount, TokenizerError> {
            Ok(TokenCount::new(encoder.count(text, framing)?, limit))
        };

        Ok(PromptTokenUsage {
            tokenizer: descriptor.prompt().tokenizer,
            limit,
            prompt: count(&resolved.prompt)?,
            negative_prompt: count(&resolved.negative_prompt)?,
            characters: resolved
                .characters
                .iter()
                .map(|character| {
                    Ok(CharacterTokenUsage {
                        index: character.index,
                        prompt: count(&character.prompt)?,
                        negative_prompt: count(&character.negative_prompt)?,
                    })
                })
                .collect::<Result<_, TokenizerError>>()?,
        })
    }
}

impl GenerateImageStreamRequest {
    /// Counts the tokens of every prompt field this request will send.
    ///
    /// # Errors
    ///
    /// Returns normalization errors or tokenizer errors, including a missing encoder.
    pub fn count_tokens(
        &self,
        tokenizers: &Tokenizers,
    ) -> Result<PromptTokenUsage, crate::BridgeError> {
        self.base.count_tokens(tokenizers)
    }

    /// Counts prompt tokens under an explicit framing policy.
    ///
    /// # Errors
    ///
    /// Returns normalization errors or tokenizer errors, including a missing encoder.
    pub fn count_tokens_with(
        &self,
        tokenizers: &Tokenizers,
        framing: FramingPolicy,
    ) -> Result<PromptTokenUsage, crate::BridgeError> {
        self.base.count_tokens_with(tokenizers, framing)
    }
}


#[cfg(test)]
mod tests;
