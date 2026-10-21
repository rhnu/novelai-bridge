//! Qwen byte-level BPE encoder driven by the official `tokenizer.json`.
//!
//! The Qwen tokenizer definition is part of the algorithm rather than metadata: the normalizer,
//! pre-tokenization pattern, added-token table, byte mapping and merge list must all come from one
//! compatible asset set.

use std::{borrow::Cow, collections::HashMap};

use fancy_regex::Regex;
use unicode_normalization::UnicodeNormalization;
mod asset;
#[cfg(test)]
mod tests;
use super::{
    FramingPolicy, TokenId, TokenizerAsset, TokenizerError,
    bpe::ByteBpe,
    encoder::{TokenCounter, TokenSink},
};

/// Unicode normalization applied before pre-tokenization.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Normalization {
    None,
    Nfc,
}

/// Qwen encoder built from `tokenizer.json` and, optionally, `tokenizer_config.json`.
#[derive(Debug)]
#[non_exhaustive]
pub struct QwenTokenizer {
    bpe: ByteBpe,
    /// Added and special tokens matched as indivisible units.
    added_tokens: HashMap<String, TokenId>,
    /// One literal search matches earliest position, then longest token at that position.
    added_token_pattern: Option<Regex>,
    normalization: Normalization,
    pattern: Regex,
}

impl QwenTokenizer {
    /// Loads the encoder from `tokenizer.json` and an optional `tokenizer_config.json`.
    ///
    /// The config file only contributes the surrounding special-token mapping; chat templates and
    /// generation metadata play no part in image prompt tokenization.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when either file is not valid JSON, the tokenizer model is not a
    /// BPE definition, vocabulary IDs are duplicated, a merge references a symbol outside the
    /// vocabulary, or a special token collides with an ordinary vocabulary entry.
    pub fn from_assets(
        tokenizer_json: &[u8],
        tokenizer_config_json: Option<&[u8]>,
    ) -> Result<Self, TokenizerError> {
        let parsed = asset::load(tokenizer_json, tokenizer_config_json)?;
        let added_token_pattern = if parsed.added_tokens.is_empty() {
            None
        } else {
            // Asset loading sorts longest first, so leftmost-first alternation also selects
            // the longest token when several literals begin at the same position.
            let pattern = parsed
                .added_tokens
                .iter()
                .map(|(token, _)| fancy_regex::escape(token))
                .collect::<Vec<_>>()
                .join("|");
            Some(
                Regex::new(&pattern).map_err(|source| TokenizerError::InvalidAssetData {
                    asset: TokenizerAsset::QwenTokenizer,
                    context: "added-token literal expression",
                    source: Box::new(source),
                })?,
            )
        };
        Ok(Self {
            bpe: parsed.bpe,
            added_tokens: parsed.added_tokens.into_iter().collect(),
            added_token_pattern,
            normalization: parsed.normalization,
            pattern: parsed.pattern,
        })
    }

    /// Returns the number of vocabulary entries.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        self.bpe.vocabulary_size()
            + self
                .added_tokens
                .iter()
                .filter(|(token, _)| self.bpe.token_id(token).is_none())
                .count()
    }

    /// Encodes text into Qwen token IDs.
    ///
    /// The Qwen profile adds no framing tokens of its own, so both policies produce the same
    /// sequence; the parameter exists so callers can treat all three encoders uniformly.
    ///
    /// # Errors
    /// Returns an encoding error if regex pre-tokenization fails.
    pub fn encode(
        &self,
        text: &str,
        _framing: FramingPolicy,
    ) -> Result<Vec<TokenId>, TokenizerError> {
        let mut out = Vec::new();
        self.encode_with_added_tokens(text, &mut out)?;
        Ok(out)
    }

    /// Splits on configured added tokens, which are indivisible, then encodes the gaps.
    fn encode_with_added_tokens(
        &self,
        text: &str,
        out: &mut impl TokenSink,
    ) -> Result<(), TokenizerError> {
        let Some(pattern) = &self.added_token_pattern else {
            return self.encode_ordinary(text, out);
        };
        let mut cursor = 0;
        for matched in pattern.find_iter(text) {
            let matched = matched.map_err(|source| TokenizerError::EncodingFailed {
                asset: TokenizerAsset::QwenTokenizer,
                source: Box::new(source),
            })?;
            // Only actual complete matches split ordinary spans; partial literal prefixes must
            // remain together for BPE. The Unicode regex keeps every boundary valid UTF-8.
            self.encode_ordinary(&text[cursor..matched.start()], out)?;
            out.push(self.added_tokens[matched.as_str()]);
            cursor = matched.end();
        }
        self.encode_ordinary(&text[cursor..], out)
    }

    fn encode_ordinary(&self, text: &str, out: &mut impl TokenSink) -> Result<(), TokenizerError> {
        let normalized = match self.normalization {
            Normalization::None => Cow::Borrowed(text),
            Normalization::Nfc => Cow::Owned(text.nfc().collect::<String>()),
        };
        let mut cursor = 0;
        for piece in self.pattern.find_iter(normalized.as_ref()) {
            let piece = piece.map_err(|error| TokenizerError::EncodingFailed {
                asset: TokenizerAsset::QwenTokenizer,
                source: Box::new(error),
            })?;
            // Split(Isolated) keeps unmatched spans as well as matches.
            if cursor < piece.start() {
                self.bpe
                    .encode_piece(&normalized[cursor..piece.start()], out);
            }
            self.bpe.encode_piece(piece.as_str(), out);
            cursor = piece.end();
        }
        if cursor < normalized.len() {
            self.bpe.encode_piece(&normalized[cursor..], out);
        }
        Ok(())
    }
}

impl super::TokenEncoder for QwenTokenizer {
    fn encode(&self, text: &str, framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError> {
        Self::encode(self, text, framing)
    }

    fn count(&self, text: &str, _framing: FramingPolicy) -> Result<u32, TokenizerError> {
        let mut count = TokenCounter::default();
        self.encode_with_added_tokens(text, &mut count)?;
        Ok(count.0)
    }

    fn vocabulary_size(&self) -> usize { Self::vocabulary_size(self) }
}
