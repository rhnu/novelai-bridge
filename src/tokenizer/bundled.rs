use super::{TokenizerError, Tokenizers};

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
        Ok(CACHE.get_or_init(|| registry).clone())
    }
}

fn expand(bytes: &[u8], asset: super::TokenizerAsset) -> Result<Vec<u8>, TokenizerError> {
    use std::io::Read;
    let mut expanded = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut expanded)
        .map_err(|source| TokenizerError::AssetDecompressionFailed { asset, source })?;
    Ok(expanded)
}
