//! Qwen byte-level BPE encoder driven by the official `tokenizer.json`.
//!
//! The Qwen tokenizer definition is part of the algorithm rather than metadata: the normalizer,
//! pre-tokenization pattern, added-token table, byte mapping and merge list must all come from one
//! compatible asset set.

use fancy_regex::Regex;
use unicode_normalization::UnicodeNormalization;
mod asset;
#[cfg(test)]
mod tests;
use super::{FramingPolicy, TokenId, TokenizerAsset, TokenizerError, bpe::ByteBpe};

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
    /// Added and special tokens matched as indivisible units, longest first.
    added_tokens: Vec<(String, TokenId)>,
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
        Ok(Self {
            bpe: parsed.bpe,
            added_tokens: parsed.added_tokens,
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
        out: &mut Vec<TokenId>,
    ) -> Result<(), TokenizerError> {
        let mut rest = text;
        while !rest.is_empty() {
            // Find actual matches, not a possible first byte: false boundaries change BPE merges.
            // str::find also keeps non-ASCII text on UTF-8 boundaries.
            let matched = self
                .added_tokens
                .iter()
                .filter_map(|(token, id)| rest.find(token).map(|pos| (pos, token, *id)))
                .min_by_key(|(pos, token, _)| (*pos, std::cmp::Reverse(token.len())));
            let Some((pos, token, id)) = matched else {
                self.encode_ordinary(rest, out)?;
                break;
            };
            self.encode_ordinary(&rest[..pos], out)?;
            out.push(id);
            rest = &rest[pos + token.len()..];
        }
        Ok(())
    }

    fn encode_ordinary(&self, text: &str, out: &mut Vec<TokenId>) -> Result<(), TokenizerError> {
        let normalized = match self.normalization {
            Normalization::None => text.to_owned(),
            Normalization::Nfc => text.nfc().collect(),
        };
        let mut cursor = 0;
        for piece in self.pattern.find_iter(&normalized) {
            let piece = piece.map_err(|error| TokenizerError::EncodingFailed {
                asset: TokenizerAsset::QwenTokenizer,
                reason: error.to_string(),
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

    fn vocabulary_size(&self) -> usize { Self::vocabulary_size(self) }
}
