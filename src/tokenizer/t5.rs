//! T5 `SentencePiece` Unigram encoder.
//!
//! T5 is not a BPE tokenizer. Segmentation is global: the highest-scoring complete path through
//! the input is selected with Viterbi, so a locally longest piece is not necessarily part of the
//! best path. Replacing this with whitespace splitting or longest-match lookup would produce
//! different IDs and different counts.
//!
//! The `spiece.model` asset is a protobuf `ModelProto`. This module reads pieces, trainer and
//! normalizer settings with a small wire-format reader rather than a protobuf codegen dependency.

use std::collections::HashMap;

mod asset;
mod normalizer;
#[cfg(test)]
mod tests;
mod wire;
use normalizer::Normalizer;

use super::{
    error::TokenizerError,
    profile::{FramingPolicy, TokenId},
};

/// Piece types as declared by `ModelProto.SentencePiece.Type`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PieceType {
    Normal,
    Unknown,
    Control,
    UserDefined,
    Byte,
    Unused,
}

#[derive(Clone, Debug)]
struct Piece {
    text: String,
    score: f32,
    kind: PieceType,
}

/// T5 encoder built from a `SentencePiece` model asset.
#[derive(Debug)]
#[non_exhaustive]
pub struct T5Tokenizer {
    pieces: Vec<Piece>,
    /// Piece text to id, restricted to pieces segmentation may emit.
    lookup: HashMap<String, TokenId>,
    /// Longest segmentable piece in bytes, used to bound the Viterbi inner loop.
    max_piece_len: usize,
    unk_id: TokenId,
    eos_id: TokenId,
    normalizer: Normalizer,
    user_symbols: Vec<String>,
    unknown_score: f32,
    user_score: f32,
}

impl T5Tokenizer {
    /// Loads the encoder from a `spiece.model` `SentencePiece` model.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when the protobuf payload is truncated or malformed, no pieces
    /// are present, two pieces claim the same text, or no unknown piece is defined.
    pub fn from_sentencepiece_model(bytes: &[u8]) -> Result<Self, TokenizerError> {
        let parsed = asset::load(bytes)?;
        Ok(Self {
            pieces: parsed.pieces,
            lookup: parsed.lookup,
            max_piece_len: parsed.max_piece_len,
            unk_id: parsed.unk_id,
            eos_id: parsed.eos_id,
            normalizer: parsed.normalizer,
            user_symbols: parsed.user_symbols,
            unknown_score: parsed.unknown_score,
            user_score: parsed.user_score,
        })
    }

    /// Returns the number of pieces in the model.
    #[must_use]
    pub const fn vocabulary_size(&self) -> usize { self.pieces.len() }

    /// Encodes text into T5 token IDs.
    ///
    /// With [`FramingPolicy::ModelInput`] the end-of-sequence marker is appended, using the id the
    /// asset itself assigns rather than a hard-coded value.
    #[must_use]
    pub fn encode(&self, text: &str, framing: FramingPolicy) -> Vec<TokenId> {
        let normalized = self.normalizer.normalize(text, &self.user_symbols);
        let mut ids = if normalized.is_empty() {
            Vec::new()
        } else {
            self.viterbi(&normalized)
        };

        if framing.includes_framing() {
            ids.push(self.eos_id);
        }
        ids
    }

    /// Selects the highest-scoring complete segmentation with dynamic programming.
    ///
    /// Ties break on the lower piece id, which keeps the result deterministic across runs and
    /// platforms regardless of hash iteration order or floating-point comparison edge cases.
    // Upstream scores are f32, and exact score ties (not approximate equality) break on IDs.
    // Separate multiply/add preserves upstream rounding instead of introducing FMA rounding.
    #[expect(
        clippy::cast_precision_loss,
        clippy::float_cmp,
        clippy::suboptimal_flops,
        reason = "SentencePiece segmentation requires upstream f32 arithmetic and exact score ties"
    )]
    fn viterbi(&self, text: &str) -> Vec<TokenId> {
        let bytes = text.len();
        // best[i] = (score of best path ending at byte i, start byte, piece id)
        let mut best: Vec<Option<(f32, usize, TokenId)>> = vec![None; bytes + 1];
        best[0] = Some((0.0, 0, 0));

        for start in 0..bytes {
            let Some((base_score, ..)) = best[start] else {
                continue;
            };
            if !text.is_char_boundary(start) {
                continue;
            }

            let limit = (start + self.max_piece_len).min(bytes);
            let mut has_single = false;
            let next = next_char_boundary(text, start);
            for end in (start + 1)..=limit {
                if !text.is_char_boundary(end) {
                    continue;
                }
                let Some(&id) = self.lookup.get(&text[start..end]) else {
                    continue;
                };
                has_single |= end == next;
                let piece = &self.pieces[id as usize];
                let piece_score = if piece.kind == PieceType::UserDefined {
                    self.user_score * text[start..end].chars().count() as f32 + 1.0
                } else {
                    piece.score
                };
                let score = base_score + piece_score;
                let candidate = (score, start, id);
                match best[end] {
                    Some((existing, _, existing_id))
                        if existing > score || (existing == score && existing_id <= id) => {},
                    _ => best[end] = Some(candidate),
                }
            }

            // No piece covers this position: fall back to the unknown piece for one character, so
            // segmentation always reaches the end instead of failing.
            if !has_single {
                let score = base_score + self.unknown_score;
                if best[next].is_none_or(|(existing, ..)| existing < score) {
                    best[next] = Some((score, start, self.unk_id));
                }
            }
        }

        let mut ids = Vec::new();
        let mut cursor = bytes;
        while cursor > 0 {
            let Some((_, start, id)) = best[cursor] else {
                // Unreachable for well-formed input; degrade to an unknown token rather than loop.
                ids.push(self.unk_id);
                break;
            };
            ids.push(id);
            cursor = start;
        }
        ids.reverse();
        ids.dedup_by(|right, left| *right == self.unk_id && *left == self.unk_id);
        ids
    }
}

fn next_char_boundary(text: &str, from: usize) -> usize {
    let mut index = from + 1;
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index.min(text.len())
}

impl super::TokenEncoder for T5Tokenizer {
    fn encode(&self, text: &str, framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError> {
        Ok(Self::encode(self, text, framing))
    }

    fn vocabulary_size(&self) -> usize { Self::vocabulary_size(self) }
}
