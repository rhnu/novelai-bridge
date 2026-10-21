use std::collections::HashMap;

use fancy_regex::Regex;
use serde::{Deserialize, de::IgnoredAny};
use serde_json::Value;

use super::Normalization;
use crate::tokenizer::{
    TokenId, TokenizerAsset, TokenizerError,
    bpe::{ByteBpe, byte_to_unicode},
};

pub(super) struct QwenAsset {
    pub(super) bpe: ByteBpe,
    pub(super) added_tokens: Vec<(String, TokenId)>,
    pub(super) normalization: Normalization,
    pub(super) pattern: Regex,
}

pub(super) fn load(
    tokenizer_json: &[u8],
    tokenizer_config_json: Option<&[u8]>,
) -> Result<QwenAsset, TokenizerError> {
    let root: Value = serde_json::from_slice(tokenizer_json).map_err(|source| {
        TokenizerError::InvalidAssetData {
            asset: TokenizerAsset::QwenTokenizer,
            context: "JSON document",
            source: Box::new(source),
        }
    })?;
    validate_structure(&root)?;

    let model = root
        .get("model")
        .ok_or_else(|| malformed(TokenizerAsset::QwenTokenizer, "missing `model`"))?;
    if let Some(kind) = model.get("type").and_then(Value::as_str)
        && kind != "BPE"
    {
        return Err(malformed(
            TokenizerAsset::QwenTokenizer,
            &format!("expected a BPE model definition, found {kind:?}"),
        ));
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
            return Err(malformed(
                TokenizerAsset::QwenTokenizer,
                "missing byte alphabet symbol",
            ));
        }
    }
    // Longest match wins, so a token that is a prefix of another never shadows it.
    added_tokens.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));

    Ok(QwenAsset {
        bpe: ByteBpe::new(vocab, merge_pairs, None),
        added_tokens,
        normalization: parse_normalization(&root)?,
        pattern: parse_pattern(&root)?,
    })
}

fn malformed(asset: TokenizerAsset, reason: &str) -> TokenizerError {
    TokenizerError::MalformedAsset {
        asset,
        reason: reason.to_owned(),
    }
}

fn deserialize_value<'de, T: Deserialize<'de>>(
    value: &'de Value,
    asset: TokenizerAsset,
    context: &'static str,
) -> Result<T, TokenizerError> {
    T::deserialize(value).map_err(|source| TokenizerError::InvalidAssetData {
        asset,
        context,
        source: Box::new(source),
    })
}

fn validate_object(
    value: &Value,
    asset: TokenizerAsset,
    context: &'static str,
) -> Result<(), TokenizerError> {
    if value.is_object() {
        return Ok(());
    }
    // Ask serde for its original type error only on the invalid path; do not clone valid assets.
    deserialize_value::<HashMap<String, Value>>(value, asset, context).map(|_| ())
}

fn parse_array<'a>(
    value: &'a Value,
    asset: TokenizerAsset,
    context: &'static str,
) -> Result<&'a [Value], TokenizerError> {
    deserialize_value::<Vec<IgnoredAny>>(value, asset, context)?;
    Ok(value.as_array().expect("deserialized an array"))
}

fn validate_fields<'de, T: Deserialize<'de>>(
    value: &'de Value,
    keys: &[&'static str],
    asset: TokenizerAsset,
) -> Result<(), TokenizerError> {
    for key in keys {
        if let Some(field) = value.get(key) {
            deserialize_value::<T>(field, asset, key)?;
        }
    }
    Ok(())
}

/// Checks known structural types before checking supported behavior. Unknown metadata is harmless,
/// but a present table or flag with the wrong type must never be treated as an omitted default.
fn validate_structure(root: &Value) -> Result<(), TokenizerError> {
    let asset = TokenizerAsset::QwenTokenizer;
    validate_object(root, asset, "JSON document")?;
    validate_fields::<String>(root, &["version"], asset)?;
    for key in [
        "model",
        "pre_tokenizer",
        "decoder",
        "normalizer",
        "post_processor",
    ] {
        if let Some(value) = root.get(key) {
            if value.is_null() && matches!(key, "normalizer" | "post_processor") {
                continue;
            }
            validate_object(value, asset, key)?;
            validate_fields::<String>(value, &["type"], asset)?;
        }
    }
    if let Some(model) = root.get("model") {
        validate_fields::<bool>(
            model,
            &["byte_fallback", "ignore_merges", "fuse_unk"],
            asset,
        )?;
        validate_fields::<Option<f64>>(model, &["dropout"], asset)?;
        validate_fields::<Option<String>>(
            model,
            &[
                "unk_token",
                "continuing_subword_prefix",
                "end_of_word_suffix",
            ],
            asset,
        )?;
    }
    if let Some(pre) = root.get("pre_tokenizer")
        && let Some(parts) = pre.get("pretokenizers")
    {
        let parts = parse_array(parts, asset, "pretokenizers")?;
        for part in parts {
            validate_object(part, asset, "pre-tokenizer entry")?;
            validate_fields::<String>(part, &["type", "behavior"], asset)?;
            validate_fields::<bool>(
                part,
                &["invert", "add_prefix_space", "use_regex", "trim_offsets"],
                asset,
            )?;
            if let Some(pattern) = part.get("pattern") {
                validate_object(pattern, asset, "Split pattern")?;
                validate_fields::<String>(pattern, &["Regex", "String"], asset)?;
            }
        }
    }
    Ok(())
}

fn parse_vocab(model: &Value) -> Result<HashMap<String, TokenId>, TokenizerError> {
    let entries = model.get("vocab").ok_or_else(|| {
        malformed(
            TokenizerAsset::QwenTokenizer,
            "missing `model.vocab` object",
        )
    })?;

    let vocab: HashMap<String, TokenId> =
        deserialize_value(entries, TokenizerAsset::QwenTokenizer, "model.vocab")?;
    let mut by_id: HashMap<TokenId, &str> = HashMap::with_capacity(vocab.len());
    for (token, id) in &vocab {
        if let Some(first) = by_id.insert(*id, token) {
            return Err(TokenizerError::DuplicateTokenId {
                asset: TokenizerAsset::QwenTokenizer,
                id: *id,
                first: first.to_owned(),
                second: token.clone(),
            });
        }
    }
    Ok(vocab)
}

fn parse_merges(model: &Value) -> Result<Vec<(String, String)>, TokenizerError> {
    let merges = model.get("merges").ok_or_else(|| {
        malformed(
            TokenizerAsset::QwenTokenizer,
            "missing `model.merges` array",
        )
    })?;
    let merges = parse_array(merges, TokenizerAsset::QwenTokenizer, "model.merges")?;

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
            },
            Value::String(text) => text
                .split_once(' ')
                .map(|(left, right)| (left.to_owned(), right.to_owned())),
            _ => None,
        };
        let pair = pair.ok_or_else(|| {
            malformed(
                TokenizerAsset::QwenTokenizer,
                &format!("merge entry {merge} is not a pair"),
            )
        })?;
        if pair.0.is_empty() || pair.1.is_empty() {
            return Err(malformed(
                TokenizerAsset::QwenTokenizer,
                &format!("merge entry {merge} has an empty side"),
            ));
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
    let Some(value) = root.get("added_tokens") else {
        return Ok(Vec::new());
    };
    let entries = parse_array(value, TokenizerAsset::QwenTokenizer, "added_tokens")?;

    let mut added = Vec::with_capacity(entries.len());
    for entry in entries {
        validate_added_flags(entry, TokenizerAsset::QwenTokenizer)?;
        let content_value = entry.get("content").ok_or_else(|| {
            malformed(
                TokenizerAsset::QwenTokenizer,
                "added token entry has no string `content`",
            )
        })?;
        let content: &str = deserialize_value(
            content_value,
            TokenizerAsset::QwenTokenizer,
            "added token content",
        )?;
        let id_value = entry.get("id").ok_or_else(|| {
            malformed(
                TokenizerAsset::QwenTokenizer,
                &format!("added token {content:?} has a non-u32 id"),
            )
        })?;
        let id: TokenId =
            deserialize_value(id_value, TokenizerAsset::QwenTokenizer, "added token ID")?;

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
        serde_json::from_slice(config_json).map_err(|source| TokenizerError::InvalidAssetData {
            asset: TokenizerAsset::QwenTokenizerConfig,
            context: "JSON document",
            source: Box::new(source),
        })?;
    validate_object(
        &config,
        TokenizerAsset::QwenTokenizerConfig,
        "JSON document",
    )?;
    validate_fields::<bool>(
        &config,
        &[
            "add_bos_token",
            "add_eos_token",
            "add_prefix_space",
            "split_special_tokens",
        ],
        TokenizerAsset::QwenTokenizerConfig,
    )?;

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

    let Some(value) = config.get("added_tokens_decoder") else {
        return Ok(());
    };
    validate_object(
        value,
        TokenizerAsset::QwenTokenizerConfig,
        "added_tokens_decoder",
    )?;
    let entries = value.as_object().expect("validated an object");

    for (id, entry) in entries {
        validate_added_flags(entry, TokenizerAsset::QwenTokenizerConfig)?;
        let content_value = entry.get("content").ok_or_else(|| {
            malformed(
                TokenizerAsset::QwenTokenizerConfig,
                "config added token has no content",
            )
        })?;
        let content: &str = deserialize_value(
            content_value,
            TokenizerAsset::QwenTokenizerConfig,
            "added token content",
        )?;
        let id = id
            .parse::<TokenId>()
            .map_err(|source| TokenizerError::InvalidAssetData {
                asset: TokenizerAsset::QwenTokenizerConfig,
                context: "added token ID",
                source: Box::new(source),
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
                return Err(malformed(
                    TokenizerAsset::QwenTokenizerConfig,
                    "config and tokenizer disagree on added token ID",
                ));
            }
        } else {
            added.push((content.to_owned(), id));
        }
    }
    Ok(())
}

fn parse_pattern(root: &Value) -> Result<Regex, TokenizerError> {
    let pattern = root["pre_tokenizer"]["pretokenizers"][0]["pattern"]["Regex"]
        .as_str()
        .ok_or_else(|| {
            unsupported(
                TokenizerAsset::QwenTokenizer,
                "missing validated Split expression",
            )
        })?;
    Regex::new(pattern).map_err(|source| TokenizerError::InvalidAssetData {
        asset: TokenizerAsset::QwenTokenizer,
        context: "pre-tokenizer regular expression",
        source: Box::new(source),
    })
}

/// Reads the normalizer the asset declares, defaulting to none.
fn parse_normalization(root: &Value) -> Result<Normalization, TokenizerError> {
    match root.get("normalizer").filter(|value| !value.is_null()) {
        None => Ok(Normalization::None),
        Some(value) if value["type"] == "NFC" => Ok(Normalization::Nfc),
        _ => Err(unsupported(
            TokenizerAsset::QwenTokenizer,
            "only null or NFC normalization is supported",
        )),
    }
}

fn unsupported(asset: TokenizerAsset, reason: &str) -> TokenizerError {
    TokenizerError::UnsupportedAssetVersion {
        asset,
        reason: reason.to_owned(),
    }
}

fn validate_added_flags(entry: &Value, asset: TokenizerAsset) -> Result<(), TokenizerError> {
    validate_object(entry, asset, "added token entry")?;
    validate_fields::<bool>(
        entry,
        &["single_word", "lstrip", "rstrip", "normalized", "special"],
        asset,
    )?;
    for key in ["single_word", "lstrip", "rstrip", "normalized"] {
        if entry.get(key).is_some_and(|value| value != false) {
            return Err(unsupported(
                asset,
                &format!("added token flag {key} must be false"),
            ));
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
            return Err(malformed(
                TokenizerAsset::QwenTokenizer,
                "empty or duplicate added token",
            ));
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
        return Err(unsupported(
            TokenizerAsset::QwenTokenizer,
            "expected tokenizer.json version 1.0",
        ));
    }
    let model = &root["model"];
    if model["type"] != "BPE" {
        return Err(unsupported(
            TokenizerAsset::QwenTokenizer,
            "expected BPE model",
        ));
    }
    for key in ["byte_fallback", "ignore_merges", "fuse_unk"] {
        if model.get(key).is_some_and(|value| value != false) {
            return Err(unsupported(TokenizerAsset::QwenTokenizer, key));
        }
    }
    for key in ["dropout", "unk_token"] {
        if model.get(key).is_some_and(|value| !value.is_null()) {
            return Err(unsupported(TokenizerAsset::QwenTokenizer, key));
        }
    }
    for key in ["continuing_subword_prefix", "end_of_word_suffix"] {
        if model
            .get(key)
            .is_some_and(|value| !value.is_null() && value != "")
        {
            return Err(unsupported(TokenizerAsset::QwenTokenizer, key));
        }
    }
    for key in ["truncation", "padding"] {
        if root.get(key).is_some_and(|value| !value.is_null()) {
            return Err(unsupported(TokenizerAsset::QwenTokenizer, key));
        }
    }
    let pre = &root["pre_tokenizer"];
    let parts = pre["pretokenizers"].as_array().ok_or_else(|| {
        unsupported(
            TokenizerAsset::QwenTokenizer,
            "expected Split + ByteLevel pre-tokenizer",
        )
    })?;
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
            TokenizerAsset::QwenTokenizer,
            "expected isolated regex Split followed by ByteLevel without prefix/regex",
        ));
    }
    if root["decoder"]["type"] != "ByteLevel" {
        return Err(unsupported(
            TokenizerAsset::QwenTokenizer,
            "expected ByteLevel decoder",
        ));
    }
    if !root["post_processor"].is_null() && root["post_processor"]["type"] != "ByteLevel" {
        return Err(unsupported(
            TokenizerAsset::QwenTokenizer,
            "framing post-processor is unsupported",
        ));
    }
    Ok(())
}
