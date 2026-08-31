//! Versioned asset loading and the encoder registry.

use std::{collections::HashMap, sync::Arc};

use crate::{ImageTokenizerKind, Model};

use super::{
    clip::ClipTokenizer,
    error::TokenizerError,
    profile::{FramingPolicy, TokenId},
    qwen::QwenTokenizer,
    t5::T5Tokenizer,
};

/// A loaded and validated encoder.
#[non_exhaustive]
#[derive(Debug)]
pub enum ImageTokenizer {
    /// `OpenAI` CLIP byte-level BPE.
    Clip(ClipTokenizer),
    /// T5 `SentencePiece` Unigram.
    T5(T5Tokenizer),
    /// Qwen byte-level BPE.
    Qwen(QwenTokenizer),
}

impl ImageTokenizer {
    /// Returns which encoder this is.
    #[must_use]
    pub const fn kind(&self) -> ImageTokenizerKind {
        match self {
            Self::Clip(_) => ImageTokenizerKind::Clip,
            Self::T5(_) => ImageTokenizerKind::T5,
            Self::Qwen(_) => ImageTokenizerKind::Qwen,
        }
    }

    /// Encodes text into token IDs, preserving order.
    ///
    /// The returned sequence is owned, so callers can count, inspect or cache it without holding
    /// on to the tokenizer's internal tables.
    ///
    /// # Errors
    /// Returns an encoding error if pre-tokenization fails; never returns a partial sequence.
    pub fn encode(
        &self,
        text: &str,
        framing: FramingPolicy,
    ) -> Result<Vec<TokenId>, TokenizerError> {
        match self {
            Self::Clip(tokenizer) => tokenizer.encode(text, framing),
            Self::T5(tokenizer) => Ok(tokenizer.encode(text, framing)),
            Self::Qwen(tokenizer) => tokenizer.encode(text, framing),
        }
    }

    /// Returns the token count for text, which is the length of the encoded sequence.
    ///
    /// # Errors
    /// Returns the same encoding errors as [`Self::encode`].
    pub fn count(&self, text: &str, framing: FramingPolicy) -> Result<u32, TokenizerError> {
        Ok(u32::try_from(self.encode(text, framing)?.len()).unwrap_or(u32::MAX))
    }

    /// Returns the number of vocabulary entries.
    ///
    /// This is a property of the asset and has nothing to do with a model's prompt limit.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        match self {
            Self::Clip(tokenizer) => tokenizer.vocabulary_size(),
            Self::T5(tokenizer) => tokenizer.vocabulary_size(),
            Self::Qwen(tokenizer) => tokenizer.vocabulary_size(),
        }
    }
}

/// Registry mapping each [`ImageTokenizerKind`] to a loaded encoder.
///
/// Assets are parsed and validated once at load time and the parsed tables are then reused, so an
/// encode operation never re-reads or re-downloads anything. Build one registry and share it.
///
/// Only the encoders a caller actually needs have to be loaded; counting a prompt for a model
/// whose encoder is absent reports [`TokenizerError::TokenizerNotLoaded`] rather than guessing.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Tokenizers {
    encoders: HashMap<ImageTokenizerKind, Arc<ImageTokenizer>>,
}

impl Tokenizers {
    /// Loads pinned assets from `novelai-bridge-tokenizer-assets`.
    /// Parsed tables are cached globally; returned registries share encoders via `Arc`.
    ///
    /// # Errors
    /// Returns an asset error if an embedded file cannot be decoded or validated.
    #[cfg(feature = "bundled-assets")]
    pub fn bundled() -> Result<Self, TokenizerError> {
        use novelai_bridge_tokenizer_assets::{
            CLIP_MERGES_GZ, QWEN_TOKENIZER_CONFIG_GZ, QWEN_TOKENIZER_GZ, T5_SENTENCEPIECE_GZ,
        };

        static CACHE: std::sync::OnceLock<Tokenizers> = std::sync::OnceLock::new();
        static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        if let Some(cached) = CACHE.get() {
            return Ok(cached.clone());
        }
        let _guard = INIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = CACHE.get() {
            return Ok(cached.clone());
        }
        let t5 = expand(T5_SENTENCEPIECE_GZ, super::TokenizerAsset::T5SentencePiece)?;
        let qwen = expand(QWEN_TOKENIZER_GZ, super::TokenizerAsset::QwenTokenizer)?;
        let config = expand(
            QWEN_TOKENIZER_CONFIG_GZ,
            super::TokenizerAsset::QwenTokenizerConfig,
        )?;
        let registry = Self::new()
            .with_clip_merges(CLIP_MERGES_GZ)?
            .with_t5_sentencepiece(&t5)?
            .with_qwen_json(&qwen, Some(&config))?;
        let _ = CACHE.set(registry.clone());
        Ok(registry)
    }

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
        self.encoders.insert(
            ImageTokenizerKind::Clip,
            Arc::new(ImageTokenizer::Clip(tokenizer)),
        );
        Ok(self)
    }

    /// Loads the T5 encoder from a `spiece.model` `SentencePiece` model.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when the asset is inconsistent or cannot be read.
    pub fn with_t5_sentencepiece(mut self, bytes: &[u8]) -> Result<Self, TokenizerError> {
        let tokenizer = T5Tokenizer::from_sentencepiece_model(bytes)?;
        self.encoders.insert(
            ImageTokenizerKind::T5,
            Arc::new(ImageTokenizer::T5(tokenizer)),
        );
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
        self.encoders.insert(
            ImageTokenizerKind::Qwen,
            Arc::new(ImageTokenizer::Qwen(tokenizer)),
        );
        Ok(self)
    }

    /// Returns the encoder of a given kind, if loaded.
    #[must_use]
    pub fn get(&self, kind: ImageTokenizerKind) -> Option<&ImageTokenizer> {
        self.encoders.get(&kind).map(Arc::as_ref)
    }

    /// Returns the encoder a model's capabilities select, if loaded.
    #[must_use]
    pub fn for_model(&self, model: Model) -> Option<&ImageTokenizer> {
        self.get(model.descriptor().prompt().tokenizer)
    }

    /// Returns the encoder a model needs, or reports which one is missing.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError::TokenizerNotLoaded`] when the model's encoder is absent.
    pub fn require_for_model(&self, model: Model) -> Result<&ImageTokenizer, TokenizerError> {
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

#[cfg(feature = "bundled-assets")]
fn expand(bytes: &[u8], asset: super::TokenizerAsset) -> Result<Vec<u8>, TokenizerError> {
    use std::io::Read;
    let mut expanded = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut expanded)
        .map_err(|source| TokenizerError::AssetDecompressionFailed { asset, source })?;
    Ok(expanded)
}
