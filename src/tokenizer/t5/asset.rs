use std::collections::HashMap;

use super::{Piece, PieceType, TokenId, TokenizerError, normalizer::Normalizer, wire::Reader};
use crate::tokenizer::TokenizerAsset;
const EOS_PIECE: &str = "</s>";

pub(super) struct T5Asset {
    pub(super) pieces: Vec<Piece>,
    pub(super) lookup: HashMap<String, TokenId>,
    pub(super) max_piece_len: usize,
    pub(super) unk_id: TokenId,
    pub(super) eos_id: TokenId,
    pub(super) normalizer: Normalizer,
    pub(super) user_symbols: Vec<String>,
    pub(super) unknown_score: f32,
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

pub(super) fn load(bytes: &[u8]) -> Result<T5Asset, TokenizerError> {
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

    for (index, piece) in pieces.iter().enumerate() {
        let id =
            TokenId::try_from(index).map_err(|source| invalid_data("piece identifier", source))?;
        if piece.text.is_empty() || !piece.score.is_finite() || !all_pieces.insert(&piece.text) {
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
        }
        if piece.kind == PieceType::UserDefined {
            user_symbols.push(piece.text.clone());
        }
        if !piece.kind.is_segmentable() {
            continue;
        }
        lookup.insert(piece.text.clone(), id);
        max_piece_len = max_piece_len.max(piece.text.len());
    }

    let unk_id = unk_id.ok_or_else(|| malformed("model defines no unknown piece"))?;
    let eos_id = eos_id.ok_or_else(|| malformed("T5 requires an EOS control"))?;
    if !min_score.is_finite() {
        return Err(malformed("T5 requires normal pieces"));
    }
    user_symbols.sort_by_key(|symbol| std::cmp::Reverse(symbol.len()));

    Ok(T5Asset {
        pieces,
        lookup,
        max_piece_len,
        unk_id,
        eos_id,
        normalizer,
        user_symbols,
        unknown_score: min_score - 10.0,
    })
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
                                .map_err(|source| invalid_data("EOS piece text", source))?
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
    let mut score = 0f32;
    let mut kind = PieceType::Normal;

    while !reader.is_done() {
        match reader.tag()? {
            (1, 2) => {
                let raw = reader.length_delimited()?;
                text = Some(
                    std::str::from_utf8(raw)
                        .map_err(|source| invalid_data("piece text", source))?
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

pub(super) fn malformed(reason: &str) -> TokenizerError {
    TokenizerError::MalformedAsset {
        asset: TokenizerAsset::T5SentencePiece,
        reason: reason.to_owned(),
    }
}

pub(super) fn invalid_data(
    context: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> TokenizerError {
    TokenizerError::InvalidAssetData {
        asset: TokenizerAsset::T5SentencePiece,
        context,
        source: Box::new(source),
    }
}
