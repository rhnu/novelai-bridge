//! Tokenizer asset and encoding failures.

use thiserror::Error;

use crate::ImageTokenizerKind;

/// Names the upstream asset a failure refers to.
///
/// Kept separate from [`ImageTokenizerKind`] because one encoder can consume several files.
#[non_exhaustive]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TokenizerAsset {
    /// `OpenAI` CLIP `bpe_simple_vocab_16e6.txt.gz` merge table.
    ClipMerges,
    /// T5 `spiece.model` `SentencePiece` model.
    T5SentencePiece,
    /// Qwen `tokenizer.json`.
    QwenTokenizer,
    /// Qwen `tokenizer_config.json`.
    QwenTokenizerConfig,
}

impl TokenizerAsset {
    /// Returns the upstream file name this asset corresponds to.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ClipMerges => "bpe_simple_vocab_16e6.txt.gz",
            Self::T5SentencePiece => "spiece.model",
            Self::QwenTokenizer => "tokenizer.json",
            Self::QwenTokenizerConfig => "tokenizer_config.json",
        }
    }
}

impl std::fmt::Display for TokenizerAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tokenizer asset loading and encoding failures.
///
/// Asset validation is deliberately strict: an internally inconsistent asset is rejected at load
/// time rather than allowed to produce plausible but wrong token counts.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum TokenizerError {
    /// A valid asset requests behavior this implementation does not support.
    #[error("{asset} uses an unsupported configuration: {reason}")]
    UnsupportedAssetVersion {
        asset: TokenizerAsset,
        reason: String,
    },
    /// Encoding failed rather than returning a partial token count.
    #[error("{asset} encoding failed")]
    EncodingFailed {
        asset: TokenizerAsset,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A gzip-compressed asset could not be decompressed.
    #[error("{asset} could not be decompressed")]
    AssetDecompressionFailed {
        asset: TokenizerAsset,
        #[source]
        source: std::io::Error,
    },
    /// An asset was structurally invalid or missing required data.
    #[error("{asset} is malformed: {reason}")]
    MalformedAsset {
        asset: TokenizerAsset,
        reason: String,
    },
    /// Serialized asset data could not be converted to its required representation.
    #[error("{asset} has invalid {context}")]
    InvalidAssetData {
        asset: TokenizerAsset,
        context: &'static str,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// Two vocabulary entries claimed the same token ID.
    #[error("{asset} assigns id {id} to both {first} and {second}")]
    DuplicateTokenId {
        asset: TokenizerAsset,
        id: u32,
        first: String,
        second: String,
    },
    /// A merge rule referenced a symbol that is not in the vocabulary.
    #[error("{asset} merge {left:?} + {right:?} references a symbol missing from the vocabulary")]
    MissingMergeSymbol {
        asset: TokenizerAsset,
        left: String,
        right: String,
    },
    /// A special token reused an ID already held by an ordinary vocabulary entry.
    #[error("{asset} special token {token:?} collides with vocabulary id {id}")]
    SpecialTokenIdCollision {
        asset: TokenizerAsset,
        token: String,
        id: u32,
    },
    /// No encoder of the requested kind has been loaded into the registry.
    #[error("no {} tokenizer has been loaded", kind.as_str())]
    TokenizerNotLoaded { kind: ImageTokenizerKind },
}
