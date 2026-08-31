//! Qwen byte-level BPE encoder driven by the official `tokenizer.json`.
//!
//! The Qwen tokenizer definition is part of the algorithm rather than metadata: the normalizer,
//! pre-tokenization pattern, added-token table, byte mapping and merge list must all come from one
//! compatible asset set.

use std::{collections::HashMap, sync::Mutex};

use fancy_regex::Regex;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use super::{
    bpe::{MergeTable, Symbol, byte_to_unicode},
    error::{TokenizerAsset, TokenizerError},
    profile::{FramingPolicy, TokenId},
};

/// Fallback pre-tokenization pattern, used only when the asset declares no `Split` pre-tokenizer.
///
/// The pattern normally comes from the asset, because it is part of the algorithm rather than
/// metadata: Qwen revisions have changed whether combining marks join letters and how digit runs
/// are grouped. Either way it needs lookahead (`\s+(?!\S)` keeps a trailing run of spaces attached
/// to the following token), which is why this crate uses `fancy-regex` rather than `regex`.
const FALLBACK_PATTERN: &str = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?[\p{L}\p{M}]+|\p{N}| ?[^\s\p{L}\p{M}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";

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
    vocab: HashMap<String, TokenId>,
    merges: MergeTable,
    byte_table: [char; 256],
    /// Added and special tokens matched as indivisible units, longest first.
    added_tokens: Vec<(String, TokenId)>,
    normalization: Normalization,
    pattern: Regex,
    cache: Mutex<HashMap<String, Vec<TokenId>>>,
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
        let root: Value = serde_json::from_slice(tokenizer_json).map_err(|error| {
            TokenizerError::MalformedAsset {
                asset: TokenizerAsset::QwenTokenizer,
                reason: format!("not valid JSON: {error}"),
            }
        })?;

        let model = root
            .get("model")
            .ok_or_else(|| malformed("missing `model`"))?;
        if let Some(kind) = model.get("type").and_then(Value::as_str)
            && kind != "BPE"
        {
            return Err(malformed(&format!(
                "expected a BPE model definition, found {kind:?}"
            )));
        }

        let vocab = parse_vocab(model)?;
        let merge_pairs = parse_merges(model)?;
        validate_merges(&merge_pairs, &vocab)?;

        let mut added_tokens = parse_added_tokens(&root, &vocab)?;
        if let Some(config) = tokenizer_config_json {
            merge_config_special_tokens(config, &vocab, &mut added_tokens)?;
        }
        validate_added_ids(&vocab, &added_tokens)?;
        validate_profile(&root)?;
        for symbol in byte_to_unicode() {
            if !vocab.contains_key(&symbol.to_string()) {
                return Err(malformed("missing byte alphabet symbol"));
            }
        }
        // Longest match wins, so a token that is a prefix of another never shadows it.
        added_tokens.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

        Ok(Self {
            merges: MergeTable::from_ranked_pairs(merge_pairs),
            vocab,
            byte_table: byte_to_unicode(),
            added_tokens,
            normalization: parse_normalization(&root)?,
            pattern: parse_pattern(&root)?,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Returns the number of vocabulary entries.
    #[must_use]
    pub fn vocabulary_size(&self) -> usize {
        self.vocab.len()
            + self
                .added_tokens
                .iter()
                .filter(|(token, _)| !self.vocab.contains_key(token))
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
                self.encode_piece(&normalized[cursor..piece.start()], out);
            }
            self.encode_piece(piece.as_str(), out);
            cursor = piece.end();
        }
        if cursor < normalized.len() {
            self.encode_piece(&normalized[cursor..], out);
        }
        Ok(())
    }

    fn encode_piece(&self, piece: &str, out: &mut Vec<TokenId>) {
        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(piece)
        {
            out.extend_from_slice(cached);
            return;
        }

        let symbols: Vec<Symbol> = piece
            .bytes()
            .map(|byte| {
                self.merges
                    .symbol(self.byte_table[byte as usize].to_string())
            })
            .collect();

        let merged = self.merges.merge(symbols);
        let mut ids = Vec::with_capacity(merged.len());
        for symbol in merged {
            ids.push(self.vocab[symbol.text()]);
        }

        out.extend_from_slice(&ids);
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= 4096 {
                cache.clear();
            }
            cache.insert(piece.to_owned(), ids);
        }
    }
}

fn malformed(reason: &str) -> TokenizerError {
    TokenizerError::MalformedAsset {
        asset: TokenizerAsset::QwenTokenizer,
        reason: reason.to_owned(),
    }
}

fn parse_vocab(model: &Value) -> Result<HashMap<String, TokenId>, TokenizerError> {
    let entries = model
        .get("vocab")
        .and_then(Value::as_object)
        .ok_or_else(|| malformed("missing `model.vocab` object"))?;

    let mut vocab = HashMap::with_capacity(entries.len());
    let mut by_id: HashMap<TokenId, String> = HashMap::with_capacity(entries.len());
    for (token, id) in entries {
        let id = id
            .as_u64()
            .and_then(|id| TokenId::try_from(id).ok())
            .ok_or_else(|| malformed(&format!("vocabulary entry {token:?} has a non-u32 id")))?;
        if let Some(first) = by_id.insert(id, token.clone()) {
            return Err(TokenizerError::DuplicateTokenId {
                asset: TokenizerAsset::QwenTokenizer,
                id,
                first,
                second: token.clone(),
            });
        }
        vocab.insert(token.clone(), id);
    }
    Ok(vocab)
}

fn parse_merges(model: &Value) -> Result<Vec<(String, String)>, TokenizerError> {
    let merges = model
        .get("merges")
        .and_then(Value::as_array)
        .ok_or_else(|| malformed("missing `model.merges` array"))?;

    let mut pairs = Vec::with_capacity(merges.len());
    for merge in merges {
        // Newer exports store each merge as a two-element array; older ones as a space-separated
        // string. Both appear in the wild, so accept either rather than pinning one export tool.
        let pair = match merge {
            Value::Array(parts) if parts.len() == 2 => {
                match (parts[0].as_str(), parts[1].as_str()) {
                    (Some(left), Some(right)) => Some((left.to_owned(), right.to_owned())),
                    _ => None,
                }
            }
            Value::String(text) => text
                .split_once(' ')
                .map(|(left, right)| (left.to_owned(), right.to_owned())),
            _ => None,
        };
        let pair = pair.ok_or_else(|| malformed(&format!("merge entry {merge} is not a pair")))?;
        if pair.0.is_empty() || pair.1.is_empty() {
            return Err(malformed(&format!("merge entry {merge} has an empty side")));
        }
        pairs.push(pair);
    }
    Ok(pairs)
}

/// Rejects merges whose symbols or result are absent from the vocabulary.
///
/// Such an asset would silently produce plausible but wrong counts, so it fails at load time.
fn validate_merges(
    pairs: &[(String, String)],
    vocab: &HashMap<String, TokenId>,
) -> Result<(), TokenizerError> {
    for (left, right) in pairs {
        let merged = format!("{left}{right}");
        if !vocab.contains_key(left) || !vocab.contains_key(right) || !vocab.contains_key(&merged) {
            return Err(TokenizerError::MissingMergeSymbol {
                asset: TokenizerAsset::QwenTokenizer,
                left: left.clone(),
                right: right.clone(),
            });
        }
    }
    Ok(())
}

fn parse_added_tokens(
    root: &Value,
    vocab: &HashMap<String, TokenId>,
) -> Result<Vec<(String, TokenId)>, TokenizerError> {
    let Some(entries) = root.get("added_tokens").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };

    let mut added = Vec::with_capacity(entries.len());
    for entry in entries {
        validate_added_flags(entry)?;
        let content = entry
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed("added token entry has no string `content`"))?;
        let id = entry
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| TokenId::try_from(id).ok())
            .ok_or_else(|| malformed(&format!("added token {content:?} has a non-u32 id")))?;

        // An added token may legitimately repeat its vocabulary entry, but it must not claim a
        // different id than the vocabulary already assigns to the same text.
        if let Some(existing) = vocab.get(content)
            && *existing != id
        {
            return Err(TokenizerError::SpecialTokenIdCollision {
                asset: TokenizerAsset::QwenTokenizer,
                token: content.to_owned(),
                id: *existing,
            });
        }

        added.push((content.to_owned(), id));
    }
    Ok(added)
}

/// Folds the config file's special tokens into the added-token table.
fn merge_config_special_tokens(
    config_json: &[u8],
    vocab: &HashMap<String, TokenId>,
    added: &mut Vec<(String, TokenId)>,
) -> Result<(), TokenizerError> {
    let config: Value =
        serde_json::from_slice(config_json).map_err(|error| TokenizerError::MalformedAsset {
            asset: TokenizerAsset::QwenTokenizerConfig,
            reason: format!("not valid JSON: {error}"),
        })?;

    for key in [
        "add_bos_token",
        "add_eos_token",
        "add_prefix_space",
        "split_special_tokens",
    ] {
        if config.get(key).is_some_and(|value| value != false) {
            return Err(TokenizerError::UnsupportedAssetVersion {
                asset: TokenizerAsset::QwenTokenizerConfig,
                reason: format!("{key} must be false"),
            });
        }
    }

    let Some(entries) = config
        .get("added_tokens_decoder")
        .and_then(Value::as_object)
    else {
        return Ok(());
    };

    for (id, entry) in entries {
        validate_added_flags(entry)?;
        let content = entry
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed("config added token has no content"))?;
        let id = id
            .parse::<TokenId>()
            .map_err(|error| TokenizerError::MalformedAsset {
                asset: TokenizerAsset::QwenTokenizerConfig,
                reason: format!("added_tokens_decoder key {id:?} is not a token id: {error}"),
            })?;

        if let Some(existing) = vocab.get(content)
            && *existing != id
        {
            return Err(TokenizerError::SpecialTokenIdCollision {
                asset: TokenizerAsset::QwenTokenizerConfig,
                token: content.to_owned(),
                id: *existing,
            });
        }
        if let Some((_, existing)) = added.iter().find(|(token, _)| token == content) {
            if *existing != id {
                return Err(malformed("config and tokenizer disagree on added token ID"));
            }
        } else {
            added.push((content.to_owned(), id));
        }
    }
    Ok(())
}

/// Reads the pre-tokenization pattern the asset declares.
///
/// Falls back to the documented Qwen pattern only when no `Split` pre-tokenizer is present, so a
/// revision that changes its own splitting rules is honoured rather than silently overridden.
fn parse_pattern(root: &Value) -> Result<Regex, TokenizerError> {
    fn split_pattern(value: &Value) -> Option<&str> {
        match value.get("type").and_then(Value::as_str)? {
            "Split" => value.get("pattern")?.get("Regex")?.as_str(),
            "Sequence" => value
                .get("pretokenizers")
                .and_then(Value::as_array)?
                .iter()
                .find_map(split_pattern),
            _ => None,
        }
    }

    let pattern = root
        .get("pre_tokenizer")
        .and_then(split_pattern)
        .unwrap_or(FALLBACK_PATTERN);

    Regex::new(pattern).map_err(|error| {
        malformed(&format!(
            "pre-tokenizer pattern {pattern:?} is not a supported expression: {error}"
        ))
    })
}

/// Reads the normalizer the asset declares, defaulting to none.
fn parse_normalization(root: &Value) -> Result<Normalization, TokenizerError> {
    match root.get("normalizer").filter(|value| !value.is_null()) {
        None => Ok(Normalization::None),
        Some(value) if value["type"] == "NFC" => Ok(Normalization::Nfc),
        _ => Err(unsupported("only null or NFC normalization is supported")),
    }
}

fn unsupported(reason: &str) -> TokenizerError {
    TokenizerError::UnsupportedAssetVersion {
        asset: TokenizerAsset::QwenTokenizer,
        reason: reason.to_owned(),
    }
}

fn validate_added_flags(entry: &Value) -> Result<(), TokenizerError> {
    for key in ["single_word", "lstrip", "rstrip", "normalized"] {
        if entry.get(key).is_some_and(|value| value != false) {
            return Err(unsupported(&format!(
                "added token flag {key} must be false"
            )));
        }
    }
    Ok(())
}

fn validate_added_ids(
    vocab: &HashMap<String, TokenId>,
    added: &[(String, TokenId)],
) -> Result<(), TokenizerError> {
    let mut ids: HashMap<TokenId, &str> = vocab
        .iter()
        .map(|(token, id)| (*id, token.as_str()))
        .collect();
    let mut strings = std::collections::HashSet::new();
    for (token, id) in added {
        if token.is_empty() || !strings.insert(token) {
            return Err(malformed("empty or duplicate added token"));
        }
        if let Some(previous) = ids.insert(*id, token)
            && previous != token
        {
            return Err(TokenizerError::SpecialTokenIdCollision {
                asset: TokenizerAsset::QwenTokenizer,
                token: token.clone(),
                id: *id,
            });
        }
    }
    Ok(())
}

fn validate_profile(root: &Value) -> Result<(), TokenizerError> {
    if root["version"] != "1.0" {
        return Err(unsupported("expected tokenizer.json version 1.0"));
    }
    let model = &root["model"];
    if model["type"] != "BPE" {
        return Err(unsupported("expected BPE model"));
    }
    for key in ["byte_fallback", "ignore_merges", "fuse_unk"] {
        if model.get(key).is_some_and(|value| value != false) {
            return Err(unsupported(key));
        }
    }
    for key in ["dropout", "unk_token"] {
        if model.get(key).is_some_and(|value| !value.is_null()) {
            return Err(unsupported(key));
        }
    }
    for key in ["continuing_subword_prefix", "end_of_word_suffix"] {
        if model
            .get(key)
            .is_some_and(|value| !value.is_null() && value != "")
        {
            return Err(unsupported(key));
        }
    }
    for key in ["truncation", "padding"] {
        if root.get(key).is_some_and(|value| !value.is_null()) {
            return Err(unsupported(key));
        }
    }
    let pre = &root["pre_tokenizer"];
    let parts = pre["pretokenizers"]
        .as_array()
        .ok_or_else(|| unsupported("expected Split + ByteLevel pre-tokenizer"))?;
    if pre["type"] != "Sequence"
        || parts.len() != 2
        || parts[0]["type"] != "Split"
        || parts[0]["behavior"] != "Isolated"
        || parts[0]["invert"] != false
        || !parts[0]["pattern"]["Regex"].is_string()
        || parts[1]["type"] != "ByteLevel"
        || parts[1]["add_prefix_space"] != false
        || parts[1]["use_regex"] != false
    {
        return Err(unsupported(
            "expected isolated regex Split followed by ByteLevel without prefix/regex",
        ));
    }
    if root["decoder"]["type"] != "ByteLevel" {
        return Err(unsupported("expected ByteLevel decoder"));
    }
    if !root["post_processor"].is_null() && root["post_processor"]["type"] != "ByteLevel" {
        return Err(unsupported("framing post-processor is unsupported"));
    }
    Ok(())
}

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn regex_failure_is_not_a_partial_success() {
        let asset = super::super::tests::qwen_asset();
        let mut tokenizer = QwenTokenizer::from_assets(asset.to_string().as_bytes(), None).unwrap();
        tokenizer.pattern = fancy_regex::RegexBuilder::new("(a+a+)+(?>b)")
            .backtrack_limit(1)
            .seek(false)
            .build()
            .unwrap();
        assert!(matches!(
            tokenizer.encode("aaaaaaaaaab", FramingPolicy::ContentOnly),
            Err(TokenizerError::EncodingFailed { .. })
        ));
    }
}
