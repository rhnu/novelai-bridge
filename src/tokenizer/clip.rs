//! `OpenAI` CLIP byte-level BPE encoder.
//!
//! Implements the pipeline of `OpenAI`'s `SimpleTokenizer` from the public CLIP repository (MIT):
//! text cleanup, regex pre-tokenization, byte-to-Unicode mapping, ranked BPE merges and
//! vocabulary lookup.
//!
//! # Deviation from upstream
//!
//! Upstream runs `ftfy.fix_text` before HTML unescaping. `ftfy`'s mojibake repair has no Rust
//! equivalent here. NFC normalization is implemented, but quote/width/ligature, control-character
//! and mojibake repairs are omitted. Inputs that need those repairs can differ from upstream.

use std::sync::LazyLock;

use fancy_regex::Regex;
use unicode_normalization::UnicodeNormalization;
mod asset;
mod html;
use html::unescape_html;

use super::{FramingPolicy, TokenId, TokenizerAsset, TokenizerError, bpe::ByteBpe};

/// The CLIP pre-tokenization pattern, verbatim from `simple_tokenizer.py`.
///
/// It recognizes common English contractions, runs of Unicode letters, single digits and runs of
/// non-whitespace punctuation.
const CLIP_PATTERN: &str =
    r"<\|startoftext\|>|<\|endoftext\|>|'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+";

const START_OF_TEXT: &str = "<|startoftext|>";
const END_OF_TEXT: &str = "<|endoftext|>";
static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(CLIP_PATTERN).expect("CLIP pattern is a valid expression"));

/// CLIP encoder built from a validated merge asset.
#[derive(Debug)]
#[non_exhaustive]
pub struct ClipTokenizer {
    bpe: ByteBpe,
    start_of_text: TokenId,
    end_of_text: TokenId,
}

impl ClipTokenizer {
    /// Loads compressed or expanded upstream CLIP merges.
    ///
    /// # Errors
    /// Returns an asset error when the merge table cannot be decoded or validated.
    pub fn from_merges_asset(bytes: &[u8]) -> Result<Self, TokenizerError> {
        let parsed = asset::parse(bytes)?;
        Ok(Self {
            bpe: parsed.bpe,
            start_of_text: parsed.start_of_text,
            end_of_text: parsed.end_of_text,
        })
    }

    /// Returns the number of vocabulary entries.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize { self.bpe.vocabulary_size() }

    /// Encodes text into CLIP token IDs.
    ///
    /// With [`FramingPolicy::ModelInput`] the sequence is wrapped in the start-of-text and
    /// end-of-text markers, matching what the text encoder consumes.
    ///
    /// # Errors
    /// Returns an encoding error if regex pre-tokenization fails.
    pub fn encode(
        &self,
        text: &str,
        framing: FramingPolicy,
    ) -> Result<Vec<TokenId>, TokenizerError> {
        let cleaned = clean_text(text);
        let mut out = Vec::new();
        if framing.includes_framing() {
            out.push(self.start_of_text);
        }

        for word in PATTERN.find_iter(&cleaned) {
            let word = word.map_err(|error| TokenizerError::EncodingFailed {
                asset: TokenizerAsset::ClipMerges,
                source: Box::new(error),
            })?;
            self.encode_word(word.as_str(), &mut out);
        }

        if framing.includes_framing() {
            out.push(self.end_of_text);
        }
        Ok(out)
    }

    fn encode_word(&self, word: &str, output: &mut Vec<TokenId>) {
        match word {
            START_OF_TEXT => output.push(self.start_of_text),
            END_OF_TEXT => output.push(self.end_of_text),
            _ => self.bpe.encode_piece(word, output),
        }
    }
}

/// Applies the upstream text cleanup: HTML entity unescaping, whitespace collapse, lowercasing.
fn clean_text(text: &str) -> String {
    // Upstream unescapes twice, which resolves doubly-encoded entities such as `&amp;lt;`.
    let normalized: String = text.nfc().collect();
    let unescaped = unescape_html(&unescape_html(&normalized));
    let mut cleaned = String::with_capacity(unescaped.len());
    let mut pending_space = false;
    for ch in unescaped.chars() {
        if ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}') {
            pending_space = !cleaned.is_empty();
            continue;
        }
        if pending_space {
            cleaned.push(' ');
            pending_space = false;
        }
        cleaned.push(ch);
    }
    cleaned.to_lowercase()
}

impl super::TokenEncoder for ClipTokenizer {
    fn encode(&self, text: &str, framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError> {
        Self::encode(self, text, framing)
    }

    fn vocabulary_size(&self) -> usize { Self::vocabulary_size(self) }
}
