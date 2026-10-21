use serde_json::{Value, json};

use super::*;
use crate::tokenizer::bpe;

pub(super) fn qwen_asset() -> Value {
    let vocab: serde_json::Map<String, Value> = bpe::byte_to_unicode()
        .into_iter()
        .enumerate()
        .map(|(id, symbol)| (symbol.to_string(), json!(id)))
        .collect();
    json!({
        "version": "1.0", "normalizer": {"type": "NFC"}, "added_tokens": [],
        "model": {"type": "BPE", "vocab": vocab, "merges": []},
        "pre_tokenizer": {"type": "Sequence", "pretokenizers": [
            {"type": "Split", "behavior": "Isolated", "invert": false, "pattern": {"Regex": "(?s).+"}},
            {"type": "ByteLevel", "add_prefix_space": false, "use_regex": false}]},
        "decoder": {"type": "ByteLevel"}
    })
}

fn load_qwen(value: &Value) -> Result<QwenTokenizer, TokenizerError> {
    QwenTokenizer::from_assets(value.to_string().as_bytes(), None)
}

#[test]
fn qwen_preserves_unicode_and_only_splits_at_actual_added_tokens() {
    let mut asset = qwen_asset();
    asset["added_tokens"] = json!([{"content": "猫🙂", "id": 256}, {"content": "猫", "id": 257}]);
    let tokenizer = load_qwen(&asset).unwrap();
    let ordinary = load_qwen(&qwen_asset()).unwrap();
    for text in ["é🙂xyz", "<<unknown>>", "猫猫🙂🙂"] {
        let ids = tokenizer.encode(text, FramingPolicy::ContentOnly).unwrap();
        if text.starts_with('猫') {
            assert_eq!(&ids[..2], &[257, 256]);
        } else {
            assert_eq!(
                ids,
                ordinary.encode(text, FramingPolicy::ContentOnly).unwrap()
            );
        }
    }
}

#[test]
fn qwen_rejects_collisions_and_unsupported_configurations() {
    for (pointer, replacement) in [
        ("/normalizer", json!({"type":"NFKC"})),
        ("/model/byte_fallback", json!(true)),
        ("/model/dropout", json!(0.2)),
        (
            "/pre_tokenizer/pretokenizers/1/add_prefix_space",
            json!(true),
        ),
        ("/pre_tokenizer/pretokenizers/0/invert", json!(true)),
        ("/post_processor", json!({"type":"TemplateProcessing"})),
        ("/version", json!("2.0")),
    ] {
        let mut asset = qwen_asset();
        // Absent model/post-processing keys need insertion rather than pointer mutation.
        if pointer == "/model/byte_fallback" {
            asset["model"]["byte_fallback"] = replacement;
        } else if pointer == "/model/dropout" {
            asset["model"]["dropout"] = replacement;
        } else if pointer == "/post_processor" {
            asset["post_processor"] = replacement;
        } else {
            *asset.pointer_mut(pointer).unwrap() = replacement;
        }
        assert!(
            matches!(
                load_qwen(&asset),
                Err(TokenizerError::UnsupportedAssetVersion { .. })
            ),
            "{pointer}"
        );
    }
    for tokens in [
        json!([{"content":"special", "id":65}]),
        json!([{"content":"one", "id":256}, {"content":"two", "id":256}]),
        json!([{"content":"", "id":256}]),
        json!([{"content":"one", "id":256, "normalized":true}]),
    ] {
        let mut asset = qwen_asset();
        asset["added_tokens"] = tokens;
        assert!(load_qwen(&asset).is_err());
    }
    let mut asset = qwen_asset();
    asset["model"]["vocab"].as_object_mut().unwrap().remove("a");
    assert!(load_qwen(&asset).is_err());
}

#[test]
fn qwen_rejects_disagreeing_companion_config() {
    let mut asset = qwen_asset();
    asset["added_tokens"] = json!([{"content":"special", "id":256}]);
    let config = json!({"added_tokens_decoder":{"257":{"content":"special"}}});
    assert!(
        QwenTokenizer::from_assets(
            asset.to_string().as_bytes(),
            Some(config.to_string().as_bytes())
        )
        .is_err()
    );
}

#[test]
fn asset_failures_keep_merge_and_vocabulary_categories() {
    let mut dangling = qwen_asset();
    dangling["model"]["merges"] = json!([["a", "b"]]);
    assert!(matches!(
        load_qwen(&dangling),
        Err(TokenizerError::MissingMergeSymbol { .. })
    ));
    let mut duplicate = qwen_asset();
    duplicate["model"]["vocab"]["a"] = json!(0);
    assert!(matches!(
        load_qwen(&duplicate),
        Err(TokenizerError::DuplicateTokenId { id: 0, .. })
    ));
}

#[test]
fn regex_failure_is_not_a_partial_success() {
    let asset = qwen_asset();
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
