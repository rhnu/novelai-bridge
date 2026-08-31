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

mod normalizer;
use normalizer::Normalizer;

use super::{
    error::{TokenizerAsset, TokenizerError},
    profile::{FramingPolicy, TokenId},
};

/// Default EOS piece name, overridable by the trainer spec.
const EOS_PIECE: &str = "</s>";

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

impl PieceType {
    fn from_wire(value: u64) -> Result<Self, TokenizerError> {
        Ok(match value {
            1 => Self::Normal,
            2 => Self::Unknown,
            3 => Self::Control,
            4 => Self::UserDefined,
            5 => Self::Unused,
            6 => Self::Byte,
            _ => return Err(malformed("unknown piece type")),
        })
    }

    /// Returns `true` when the piece may be produced by ordinary segmentation.
    const fn is_segmentable(self) -> bool {
        matches!(self, Self::Normal | Self::UserDefined)
    }
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
    eos_id: Option<TokenId>,
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
        let (pieces, normalizer, eos_piece) = parse_model_proto(bytes)?;
        if pieces.is_empty() {
            return Err(malformed("model declares no pieces"));
        }

        let mut lookup: HashMap<String, TokenId> = HashMap::with_capacity(pieces.len());
        let mut max_piece_len = 0usize;
        let mut unk_id = None;
        let mut eos_id = None;
        let mut all_pieces = std::collections::HashSet::new();
        let mut user_symbols = Vec::new();
        let mut min_score = f32::INFINITY;
        let mut max_score = f32::NEG_INFINITY;

        for (index, piece) in pieces.iter().enumerate() {
            let id = TokenId::try_from(index).map_err(|_| malformed("piece count exceeds u32"))?;
            if piece.text.is_empty() || !piece.score.is_finite() || !all_pieces.insert(&piece.text)
            {
                return Err(malformed("empty/duplicate piece or non-finite score"));
            }
            if piece.kind == PieceType::Byte {
                return Err(malformed(
                    "T5 profile does not support byte fallback pieces",
                ));
            }
            if piece.kind == PieceType::Unknown {
                if unk_id.is_some() {
                    return Err(malformed("multiple unknown pieces"));
                }
                unk_id = Some(id);
            }
            // Special-token ids are read from the asset itself; hard-coding them from a different
            // T5 checkpoint would silently mis-encode a model that renumbered its extra ids.
            if piece.text == eos_piece && piece.kind == PieceType::Control {
                eos_id = Some(id);
            }
            if piece.kind == PieceType::Normal {
                min_score = min_score.min(piece.score);
                max_score = max_score.max(piece.score);
            }
            if piece.kind == PieceType::UserDefined {
                user_symbols.push(piece.text.clone());
            }
            if !piece.kind.is_segmentable() {
                continue;
            }
            if let Some(previous) = lookup.insert(piece.text.clone(), id) {
                return Err(TokenizerError::DuplicateTokenId {
                    asset: TokenizerAsset::T5SentencePiece,
                    id: previous,
                    first: piece.text.clone(),
                    second: piece.text.clone(),
                });
            }
            max_piece_len = max_piece_len.max(piece.text.len());
        }

        let unk_id = unk_id.ok_or_else(|| malformed("model defines no unknown piece"))?;
        if eos_id.is_none() || !min_score.is_finite() {
            return Err(malformed("T5 requires an EOS control and normal pieces"));
        }
        user_symbols.sort_by_key(|symbol| std::cmp::Reverse(symbol.len()));

        Ok(Self {
            pieces,
            lookup,
            max_piece_len,
            unk_id,
            eos_id,
            normalizer,
            user_symbols,
            unknown_score: min_score - 10.0,
            user_score: max_score,
        })
    }

    /// Returns the number of pieces in the model.
    #[must_use]
    pub const fn vocabulary_size(&self) -> usize {
        self.pieces.len()
    }

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

        if framing.includes_framing()
            && let Some(eos) = self.eos_id
        {
            ids.push(eos);
        }
        ids
    }

    /// Selects the highest-scoring complete segmentation with dynamic programming.
    ///
    /// Ties break on the lower piece id, which keeps the result deterministic across runs and
    /// platforms regardless of hash iteration order or floating-point comparison edge cases.
    // Upstream scores are f32, and exact score ties (not approximate equality) break on IDs.
    // Separate multiply/add preserves upstream rounding instead of introducing FMA rounding.
    #[allow(
        clippy::cast_precision_loss,
        clippy::float_cmp,
        clippy::suboptimal_flops
    )]
    fn viterbi(&self, text: &str) -> Vec<TokenId> {
        let bytes = text.len();
        // best[i] = (score of best path ending at byte i, start byte, piece id)
        let mut best: Vec<Option<(f32, usize, TokenId)>> = vec![None; bytes + 1];
        best[0] = Some((0.0, 0, 0));

        for start in 0..bytes {
            let Some((base_score, _, _)) = best[start] else {
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
                        if existing > score || (existing == score && existing_id <= id) => {}
                    _ => best[end] = Some(candidate),
                }
            }

            // No piece covers this position: fall back to the unknown piece for one character, so
            // segmentation always reaches the end instead of failing.
            if !has_single {
                let score = base_score + self.unknown_score;
                if best[next].is_none_or(|(existing, _, _)| existing < score) {
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

fn malformed(reason: &str) -> TokenizerError {
    TokenizerError::MalformedAsset {
        asset: TokenizerAsset::T5SentencePiece,
        reason: reason.to_owned(),
    }
}

// --- Minimal protobuf reader -------------------------------------------------------------------
//
// `ModelProto` field 1 is a repeated `SentencePiece`, whose fields are `piece` (1, string),
// `score` (2, float) and `type` (3, enum). Fields 2/3 carry trainer/normalizer settings;
// unrelated training metadata is skipped by wire type.

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    const fn is_done(&self) -> bool {
        self.position >= self.bytes.len()
    }

    fn varint(&mut self) -> Result<u64, TokenizerError> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = *self
                .bytes
                .get(self.position)
                .ok_or_else(|| malformed("truncated varint"))?;
            self.position += 1;
            if shift == 63 && byte > 1 {
                return Err(malformed("varint exceeds 64 bits"));
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 64 {
                return Err(malformed("varint exceeds 64 bits"));
            }
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], TokenizerError> {
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| malformed("length-delimited field runs past the end of the model"))?;
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }

    fn length_delimited(&mut self) -> Result<&'a [u8], TokenizerError> {
        let length = usize::try_from(self.varint()?)
            .map_err(|_| malformed("length-delimited field is longer than usize"))?;
        self.take(length)
    }

    /// Reads a tag and returns `(field number, wire type)`.
    fn tag(&mut self) -> Result<(u64, u8), TokenizerError> {
        let key = self.varint()?;
        if key >> 3 == 0 {
            return Err(malformed("protobuf field zero"));
        }
        let wire_type = u8::try_from(key & 0x7).expect("three bits fit in u8");
        Ok((key >> 3, wire_type))
    }

    /// Skips a field whose contents are not needed.
    fn skip(&mut self, wire_type: u8) -> Result<(), TokenizerError> {
        match wire_type {
            0 => {
                self.varint()?;
            }
            1 => {
                self.take(8)?;
            }
            2 => {
                self.length_delimited()?;
            }
            5 => {
                self.take(4)?;
            }
            other => return Err(malformed(&format!("unsupported wire type {other}"))),
        }
        Ok(())
    }
}

fn parse_model_proto(bytes: &[u8]) -> Result<(Vec<Piece>, Normalizer, String), TokenizerError> {
    let mut reader = Reader::new(bytes);
    let mut pieces = Vec::new();
    let mut normalizer = Normalizer::default();
    let mut suffix = false;
    let mut eos_piece = EOS_PIECE.to_owned();

    while !reader.is_done() {
        let (field, wire_type) = reader.tag()?;
        match (field, wire_type) {
            (1, 2) => pieces.push(parse_piece(reader.length_delimited()?)?),
            (2, 2) => {
                let mut trainer = Reader::new(reader.length_delimited()?);
                while !trainer.is_done() {
                    match trainer.tag()? {
                        (3, 0) => {
                            if trainer.varint()? != 1 {
                                return Err(malformed("T5 requires a Unigram model"));
                            }
                        }
                        (24, 0) => suffix = trainer.varint()? != 0,
                        (35, 0) => {
                            if trainer.varint()? != 0 {
                                return Err(malformed("T5 byte fallback is unsupported"));
                            }
                        }
                        (47, 2) => {
                            std::str::from_utf8(trainer.length_delimited()?)
                                .map_err(|_| malformed("invalid EOS piece"))?
                                .clone_into(&mut eos_piece);
                        }
                        (_, wire) => trainer.skip(wire)?,
                    }
                }
            }
            (3, 2) => normalizer = Normalizer::parse(reader.length_delimited()?)?,
            (_, wire) => reader.skip(wire)?,
        }
    }

    normalizer.suffix = suffix;
    Ok((pieces, normalizer, eos_piece))
}

fn parse_piece(bytes: &[u8]) -> Result<Piece, TokenizerError> {
    let mut reader = Reader::new(bytes);
    let mut text = None;
    let mut score = 0.0f32;
    let mut kind = PieceType::Normal;

    while !reader.is_done() {
        match reader.tag()? {
            (1, 2) => {
                let raw = reader.length_delimited()?;
                text = Some(
                    std::str::from_utf8(raw)
                        .map_err(|error| malformed(&format!("piece is not valid UTF-8: {error}")))?
                        .to_owned(),
                );
            }
            (2, 5) => {
                let raw = reader.take(4)?;
                score = f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            }
            (3, 0) => {
                let raw = reader.varint()?;
                kind = PieceType::from_wire(raw)?;
            }
            (_, wire_type) => reader.skip(wire_type)?,
        }
    }

    Ok(Piece {
        text: text.ok_or_else(|| malformed("piece entry has no text"))?,
        score,
        kind,
    })
}
