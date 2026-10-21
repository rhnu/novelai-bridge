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
fn qwen_added_tokens_choose_earliest_then_longest_literal() {
    let mut asset = qwen_asset();
    asset["added_tokens"] = json!([
        {"content": "猫🙂", "id": 256},
        {"content": "🙂", "id": 257},
        {"content": "猫", "id": 258},
        {"content": "[a].*", "id": 259},
        {"content": "a", "id": 97},
        {"content": "never-present-token", "id": 261}
    ]);
    let tokenizer = load_qwen(&asset).unwrap();
    assert_eq!(
        tokenizer
            .encode("🙂猫🙂猫[a].*a", FramingPolicy::ContentOnly)
            .unwrap(),
        [257, 256, 258, 259, 97],
    );
}

#[test]
fn qwen_false_special_prefixes_do_not_break_ordinary_merges() {
    let mut asset = qwen_asset();
    asset["normalizer"] = Value::Null;
    asset["model"]["vocab"]["ab"] = json!(256);
    asset["model"]["vocab"]["abc"] = json!(257);
    asset["model"]["merges"] = json!([["a", "b"], ["ab", "c"]]);
    asset["added_tokens"] = json!([
        {"content": "bc!", "id": 258},
        {"content": "<|special|>", "id": 259}
    ]);
    let tokenizer = load_qwen(&asset).unwrap();
    assert_eq!(
        tokenizer
            .encode("abc<|special|>abc", FramingPolicy::ContentOnly)
            .unwrap(),
        [257, 259, 257],
    );
}

#[test]
fn qwen_repeated_special_tokens_preserve_ordinary_unicode_gaps() {
    let mut asset = qwen_asset();
    asset["normalizer"] = Value::Null;
    asset["added_tokens"] = json!([
        {"content": "<|special|>", "id": 256},
        {"content": "unmatched-special-token", "id": 257}
    ]);
    let tokenizer = load_qwen(&asset).unwrap();
    let text = "e\u{301}<|special|>🙂<|special|>".repeat(128);
    let expected: Vec<_> = [101, 204, 129, 256, 240, 159, 153, 130, 256]
        .into_iter()
        .cycle()
        .take(9 * 128)
        .collect();
    assert_eq!(
        tokenizer.encode(&text, FramingPolicy::ContentOnly).unwrap(),
        expected
    );
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
fn qwen_rejects_present_added_token_tables_with_wrong_types_and_retains_causes() {
    use std::error::Error as _;

    for wrong in [
        Value::Null,
        json!(false),
        json!(23),
        json!("tokens"),
        json!({}),
    ] {
        let mut asset = qwen_asset();
        asset["added_tokens"] = wrong;
        let error = load_qwen(&asset).unwrap_err();
        assert!(matches!(
            error,
            TokenizerError::InvalidAssetData {
                asset: TokenizerAsset::QwenTokenizer,
                context: "added_tokens",
                ..
            }
        ));
        assert!(error.source().unwrap().is::<serde_json::Error>());
    }

    let asset = qwen_asset().to_string();
    for wrong in [
        Value::Null,
        json!(false),
        json!(23),
        json!("tokens"),
        json!([]),
    ] {
        let config = json!({"added_tokens_decoder": wrong}).to_string();
        let error =
            QwenTokenizer::from_assets(asset.as_bytes(), Some(config.as_bytes())).unwrap_err();
        assert!(matches!(
            error,
            TokenizerError::InvalidAssetData {
                asset: TokenizerAsset::QwenTokenizerConfig,
                context: "added_tokens_decoder",
                ..
            }
        ));
        assert!(error.source().unwrap().is::<serde_json::Error>());
    }
}

#[test]
fn qwen_checks_known_structural_types_without_treating_them_as_defaults() {
    use std::error::Error as _;

    for (pointer, wrong) in [
        ("/version", json!(1)),
        ("/model", json!([])),
        ("/model/vocab", json!([])),
        ("/model/vocab/a", json!("1")),
        ("/model/merges", json!({})),
        ("/normalizer", json!([])),
        ("/normalizer/type", json!(true)),
        ("/pre_tokenizer", json!([])),
        ("/pre_tokenizer/pretokenizers", json!({})),
        ("/pre_tokenizer/pretokenizers/0", json!([])),
        ("/pre_tokenizer/pretokenizers/0/invert", json!("false")),
        ("/pre_tokenizer/pretokenizers/0/pattern", json!([])),
        ("/pre_tokenizer/pretokenizers/0/pattern/Regex", json!(false)),
        ("/pre_tokenizer/pretokenizers/1/use_regex", json!(0)),
        ("/decoder", json!([])),
        ("/added_tokens", json!([{"id": 256, "content": 42}])),
        (
            "/added_tokens",
            json!([{"id": "256", "content": "special"}]),
        ),
        (
            "/added_tokens",
            json!([{"id": 256, "content": "special", "special": "true"}]),
        ),
        (
            "/added_tokens",
            json!([{"id": 256, "content": "special", "normalized": null}]),
        ),
    ] {
        let mut asset = qwen_asset();
        *asset.pointer_mut(pointer).unwrap() = wrong;
        let error = load_qwen(&asset).unwrap_err();
        assert!(
            matches!(error, TokenizerError::InvalidAssetData { .. }),
            "{pointer}: {error:?}"
        );
        assert!(
            error.source().unwrap().is::<serde_json::Error>(),
            "{pointer}"
        );
    }

    let asset = qwen_asset().to_string();
    for config in [
        json!([]),
        Value::Null,
        json!({"add_bos_token": "false"}),
        json!({"added_tokens_decoder": {"256": []}}),
        json!({"added_tokens_decoder": {"256": {"content": 42}}}),
        json!({"added_tokens_decoder": {"256": {"content": "special", "lstrip": "false"}}}),
    ] {
        let error =
            QwenTokenizer::from_assets(asset.as_bytes(), Some(config.to_string().as_bytes()))
                .unwrap_err();
        assert!(matches!(
            error,
            TokenizerError::InvalidAssetData {
                asset: TokenizerAsset::QwenTokenizerConfig,
                ..
            }
        ));
        assert!(error.source().unwrap().is::<serde_json::Error>());
    }
}

#[test]
fn qwen_missing_tables_keep_defaults_and_known_special_tokens_survive_metadata() {
    let mut asset = qwen_asset();
    asset.as_object_mut().unwrap().remove("added_tokens");
    let without_tables =
        QwenTokenizer::from_assets(asset.to_string().as_bytes(), Some(b"{}")).unwrap();
    assert_eq!(without_tables.vocabulary_size(), 256);

    asset["added_tokens"] = json!([{
        "id": 256, "content": "<|first|>", "special": true,
        "single_word": false, "lstrip": false, "rstrip": false, "normalized": false,
        "export_metadata": {"future": true},
    }]);
    asset["export_metadata"] = json!({"not_used_for_encoding": [1, 2, 3]});
    let config = json!({
        "added_tokens_decoder": {
            "256": {"content": "<|first|>", "special": true},
            "257": {"content": "<|second|>", "special": true},
        },
        "chat_template": "unused template",
        "future_metadata": {"anything": null},
    });
    let tokenizer = QwenTokenizer::from_assets(
        asset.to_string().as_bytes(),
        Some(config.to_string().as_bytes()),
    )
    .unwrap();
    assert_eq!(
        tokenizer
            .encode("<|first|><|second|>", FramingPolicy::ContentOnly)
            .unwrap(),
        [256, 257],
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

#[test]
fn json_and_regex_asset_errors_retain_sources_and_asset_identity() {
    use std::error::Error as _;
    let error = QwenTokenizer::from_assets(b"{", None).unwrap_err();
    assert!(error.source().unwrap().is::<serde_json::Error>());
    let mut asset = qwen_asset();
    asset["pre_tokenizer"]["pretokenizers"][0]["pattern"]["Regex"] = json!("(");
    let error = load_qwen(&asset).unwrap_err();
    assert!(error.source().unwrap().is::<fancy_regex::Error>());
    let asset = qwen_asset();
    let error = QwenTokenizer::from_assets(asset.to_string().as_bytes(), Some(b"{")).unwrap_err();
    assert!(matches!(
        error,
        TokenizerError::InvalidAssetData {
            asset: TokenizerAsset::QwenTokenizerConfig,
            ..
        }
    ));
    assert!(error.source().unwrap().is::<serde_json::Error>());
    let config = json!({"added_tokens_decoder":{"300":{"content":"special","lstrip":true}}});
    let error = QwenTokenizer::from_assets(
        asset.to_string().as_bytes(),
        Some(config.to_string().as_bytes()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        TokenizerError::UnsupportedAssetVersion {
            asset: TokenizerAsset::QwenTokenizerConfig,
            ..
        }
    ));
}
