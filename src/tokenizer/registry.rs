use std::{collections::HashMap, sync::Arc};

use super::{ClipTokenizer, QwenTokenizer, T5Tokenizer, TokenEncoder, TokenizerError};
use crate::{ImageTokenizerKind, Model};

#[cfg(test)]
mod tests;

/// Registry mapping each [`ImageTokenizerKind`] to a loaded encoder.
///
/// Assets are parsed and validated once at load time and the parsed tables are then reused, so an
/// encode operation never re-reads or re-downloads anything. Build one registry and share it.
///
/// Only the encoders a caller actually needs have to be loaded; counting a prompt for a model
/// whose encoder is absent reports [`TokenizerError::TokenizerNotLoaded`] rather than guessing.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct Tokenizers {
    encoders: HashMap<ImageTokenizerKind, Arc<dyn TokenEncoder>>,
}

impl Tokenizers {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads the CLIP encoder from `bpe_simple_vocab_16e6.txt.gz`, compressed or expanded.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when the asset is inconsistent or cannot be read.
    pub fn with_clip_merges(mut self, bytes: &[u8]) -> Result<Self, TokenizerError> {
        let tokenizer = ClipTokenizer::from_merges_asset(bytes)?;
        self.encoders
            .insert(ImageTokenizerKind::Clip, Arc::new(tokenizer));
        Ok(self)
    }

    /// Loads the T5 encoder from a `spiece.model` `SentencePiece` model.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when the asset is inconsistent or cannot be read.
    pub fn with_t5_sentencepiece(mut self, bytes: &[u8]) -> Result<Self, TokenizerError> {
        let tokenizer = T5Tokenizer::from_sentencepiece_model(bytes)?;
        self.encoders
            .insert(ImageTokenizerKind::T5, Arc::new(tokenizer));
        Ok(self)
    }

    /// Loads the Qwen encoder from `tokenizer.json` and an optional `tokenizer_config.json`.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when either asset is inconsistent or cannot be read.
    pub fn with_qwen_json(
        mut self,
        tokenizer_json: &[u8],
        tokenizer_config_json: Option<&[u8]>,
    ) -> Result<Self, TokenizerError> {
        let tokenizer = QwenTokenizer::from_assets(tokenizer_json, tokenizer_config_json)?;
        self.encoders
            .insert(ImageTokenizerKind::Qwen, Arc::new(tokenizer));
        Ok(self)
    }

    /// Registers a loaded encoder, replacing any previous encoder of this kind.
    pub fn register(
        &mut self,
        kind: ImageTokenizerKind,
        encoder: Arc<dyn TokenEncoder>,
    ) -> Option<Arc<dyn TokenEncoder>> {
        self.encoders.insert(kind, encoder)
    }

    /// Returns the encoder of a given kind, if loaded.
    #[must_use]
    pub fn get(&self, kind: ImageTokenizerKind) -> Option<&dyn TokenEncoder> {
        self.encoders.get(&kind).map(Arc::as_ref)
    }

    /// Returns the encoder a model's capabilities select, if loaded.
    #[must_use]
    pub fn for_model(&self, model: Model) -> Option<&dyn TokenEncoder> {
        self.get(model.descriptor().prompt().tokenizer)
    }

    /// Returns the encoder a model needs, or reports which one is missing.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError::TokenizerNotLoaded`] when the model's encoder is absent.
    pub fn require_for_model(&self, model: Model) -> Result<&dyn TokenEncoder, TokenizerError> {
        let kind = model.descriptor().prompt().tokenizer;
        self.get(kind)
            .ok_or(TokenizerError::TokenizerNotLoaded { kind })
    }

    /// Returns `true` when no encoder is loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.encoders.is_empty()
    }
}

impl std::fmt::Debug for Tokenizers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Tokenizers")
            .field("kinds", &self.encoders.keys())
            .finish()
    }
}
