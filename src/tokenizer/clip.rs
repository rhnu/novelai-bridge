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

use std::{
    collections::HashMap,
    io::Read,
    sync::{LazyLock, Mutex},
};

use fancy_regex::Regex;
use flate2::read::GzDecoder;
use unicode_normalization::UnicodeNormalization;
mod html;
use html::unescape_html;

use super::{
    bpe::{MergeTable, Symbol, byte_alphabet_order, byte_to_unicode},
    error::{TokenizerAsset, TokenizerError},
    profile::{FramingPolicy, TokenId},
};

/// The CLIP pre-tokenization pattern, verbatim from `simple_tokenizer.py`.
///
/// It recognizes common English contractions, runs of Unicode letters, single digits and runs of
/// non-whitespace punctuation.
const CLIP_PATTERN: &str =
    r"<\|startoftext\|>|<\|endoftext\|>|'s|'t|'re|'ve|'m|'ll|'d|[\p{L}]+|[\p{N}]|[^\s\p{L}\p{N}]+";

const START_OF_TEXT: &str = "<|startoftext|>";
const END_OF_TEXT: &str = "<|endoftext|>";
const WORD_SUFFIX: &str = "</w>";

/// Number of merge lines upstream keeps from the asset: it skips the header line and truncates to
/// `49152 - 256 - 2` entries so the derived vocabulary lands exactly on 49408 tokens.
const MERGE_COUNT: usize = 49152 - 256 - 2;

static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(CLIP_PATTERN).expect("CLIP pattern is a valid expression"));

/// CLIP encoder built from the public merge asset.
#[derive(Debug)]
#[non_exhaustive]
pub struct ClipTokenizer {
    vocab: HashMap<String, TokenId>,
    merges: MergeTable,
    byte_table: [char; 256],
    start_of_text: TokenId,
    end_of_text: TokenId,
    /// Word-level memoization. BPE on a repeated tag is pure, so caching it is a large win on
    /// comma-separated prompts where the same tags recur.
    cache: Mutex<HashMap<String, Vec<TokenId>>>,
}

impl ClipTokenizer {
    /// Loads the encoder from `bpe_simple_vocab_16e6.txt.gz`, gzip-compressed or already expanded.
    ///
    /// # Errors
    ///
    /// Returns [`TokenizerError`] when the asset cannot be decompressed, has too few merge lines,
    /// contains a malformed merge line, or produces a vocabulary with duplicate entries.
    pub fn from_merges_asset(bytes: &[u8]) -> Result<Self, TokenizerError> {
        let text = decompress_if_gzip(bytes)?;
        Self::from_merges_text(&text)
    }

    fn from_merges_text(text: &str) -> Result<Self, TokenizerError> {
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

        let byte_table = byte_to_unicode();
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

        Ok(Self {
            vocab,
            merges: MergeTable::from_ranked_pairs(pairs),
            byte_table,
            start_of_text,
            end_of_text,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the number of vocabulary entries.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        self.vocab.len()
    }

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
                reason: error.to_string(),
            })?;
            self.encode_word(word.as_str(), &mut out);
        }

        if framing.includes_framing() {
            out.push(self.end_of_text);
        }
        Ok(out)
    }

    fn encode_word(&self, word: &str, out: &mut Vec<TokenId>) {
        if word == START_OF_TEXT || word == END_OF_TEXT {
            out.push(self.vocab[word]);
            return;
        }
        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(word)
        {
            out.extend_from_slice(cached);
            return;
        }

        // Every byte maps into the reversible alphabet, and the final symbol carries the
        // end-of-word suffix so word-final pieces stay distinguishable from word-internal ones.
        let mut symbols: Vec<Symbol> = word
            .bytes()
            .map(|byte| {
                self.merges
                    .symbol(self.byte_table[byte as usize].to_string())
            })
            .collect();
        if let Some(last) = symbols.pop() {
            symbols.push(self.merges.symbol(format!("{}{WORD_SUFFIX}", last.text())));
        }

        let merged = self.merges.merge(symbols);
        let ids: Vec<TokenId> = merged
            .into_iter()
            .map(|symbol| self.vocab[symbol.text()])
            .collect();

        out.extend_from_slice(&ids);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(word.to_owned(), ids);
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

/// Expands the asset when it carries a gzip header, otherwise decodes it as UTF-8 text.
fn decompress_if_gzip(bytes: &[u8]) -> Result<String, TokenizerError> {
    let is_gzip = bytes.starts_with(&[0x1f, 0x8b]);
    if !is_gzip {
        return String::from_utf8(bytes.to_vec()).map_err(|error| TokenizerError::MalformedAsset {
            asset: TokenizerAsset::ClipMerges,
            reason: format!("merge asset is not valid UTF-8: {error}"),
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
