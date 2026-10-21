use std::{collections::HashMap, io::Read};

use flate2::read::GzDecoder;

use super::{END_OF_TEXT, START_OF_TEXT};
use crate::tokenizer::{
    TokenId, TokenizerAsset, TokenizerError,
    bpe::{ByteBpe, byte_alphabet_order},
};
const WORD_SUFFIX: &str = "</w>";
const MERGE_COUNT: usize = 49152 - 256 - 2;

pub(super) struct ClipAsset {
    pub(super) bpe: ByteBpe,
    pub(super) start_of_text: TokenId,
    pub(super) end_of_text: TokenId,
}

pub(super) fn parse(bytes: &[u8]) -> Result<ClipAsset, TokenizerError> {
    let text = decompress_if_gzip(bytes)?;
    // Upstream drops the first line (a version header) and keeps a fixed number of merges.
    let merge_lines: Vec<&str> = text.lines().skip(1).take(MERGE_COUNT).collect();
    if merge_lines.len() < MERGE_COUNT {
        return Err(TokenizerError::MalformedAsset {
            asset: TokenizerAsset::ClipMerges,
            reason: format!(
                "expected {MERGE_COUNT} merge lines after the header, found {}",
                merge_lines.len()
            ),
        });
    }

    let mut pairs = Vec::with_capacity(merge_lines.len());
    for line in merge_lines {
        let mut parts = line.split(' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(left), Some(right), None) if !left.is_empty() && !right.is_empty() => {
                pairs.push((left.to_owned(), right.to_owned()));
            }
            _ => {
                return Err(TokenizerError::MalformedAsset {
                    asset: TokenizerAsset::ClipMerges,
                    reason: format!("merge line {line:?} is not an exact symbol pair"),
                });
            }
        }
    }

    let mut vocab: HashMap<String, TokenId> = HashMap::new();
    let insert = |vocab: &mut HashMap<String, TokenId>, token: String| {
        let id = TokenId::try_from(vocab.len()).expect("vocabulary fits in u32");
        vocab.insert(token.clone(), id).map_or(Ok(()), |previous| {
            Err(TokenizerError::DuplicateTokenId {
                asset: TokenizerAsset::ClipMerges,
                id: previous,
                first: token.clone(),
                second: token,
            })
        })
    };

    // The vocabulary order is fixed by upstream: bare byte symbols, then the same symbols with
    // the end-of-word suffix, then every merge result, then the two framing tokens.
    for (_, symbol) in byte_alphabet_order() {
        insert(&mut vocab, symbol.to_string())?;
    }
    for (_, symbol) in byte_alphabet_order() {
        insert(&mut vocab, format!("{symbol}{WORD_SUFFIX}"))?;
    }
    for (left, right) in &pairs {
        if !vocab.contains_key(left) || !vocab.contains_key(right) {
            return Err(TokenizerError::MissingMergeSymbol {
                asset: TokenizerAsset::ClipMerges,
                left: left.clone(),
                right: right.clone(),
            });
        }
        insert(&mut vocab, format!("{left}{right}"))?;
    }
    insert(&mut vocab, START_OF_TEXT.to_owned())?;
    insert(&mut vocab, END_OF_TEXT.to_owned())?;

    let start_of_text = vocab[START_OF_TEXT];
    let end_of_text = vocab[END_OF_TEXT];

    Ok(ClipAsset {
        bpe: ByteBpe::new(vocab, pairs, Some(WORD_SUFFIX)),
        start_of_text,
        end_of_text,
    })
}

/// Expands the asset when it carries a gzip header, otherwise decodes it as UTF-8 text.
fn decompress_if_gzip(bytes: &[u8]) -> Result<String, TokenizerError> {
    let is_gzip = bytes.starts_with(&[0x1F, 0x8B]);
    if !is_gzip {
        return String::from_utf8(bytes.to_vec()).map_err(|source| {
            TokenizerError::InvalidAssetData {
                asset: TokenizerAsset::ClipMerges,
                context: "merge text",
                source: Box::new(source),
            }
        });
    }

    let mut decoder = GzDecoder::new(bytes);
    let mut text = String::new();
    decoder.read_to_string(&mut text).map_err(|source| {
        TokenizerError::AssetDecompressionFailed {
            asset: TokenizerAsset::ClipMerges,
            source,
        }
    })?;
    Ok(text)
}
