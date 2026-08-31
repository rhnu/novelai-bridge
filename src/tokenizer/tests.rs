use super::*;
use serde_json::{Value, json};

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

fn field(number: u8, data: &[u8]) -> Vec<u8> {
    assert!(data.len() < 128);
    [
        vec![number * 8 + 2, u8::try_from(data.len()).unwrap()],
        data.to_vec(),
    ]
    .concat()
}

fn piece(text: &str, score: f32, kind: u8) -> Vec<u8> {
    field(
        1,
        &[
            field(1, text.as_bytes()),
            vec![21],
            score.to_le_bytes().to_vec(),
            vec![24, kind],
        ]
        .concat(),
    )
}

fn t5_asset() -> Vec<u8> {
    [
        piece("<unk>", 0.0, 2),
        piece("</s>", 0.0, 3),
        piece("▁", -1.0, 1),
        piece("a", -1.0, 1),
        piece("b", -1.0, 1),
        piece("ab", -5.0, 1),
        piece("猫", 0.0, 4),
        piece("unused", 0.0, 5),
    ]
    .concat()
}

#[test]
fn t5_uses_viterbi_unknown_runs_user_symbols_and_normalizer_flags() {
    let bytes = t5_asset();
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&bytes).unwrap();
    assert_eq!(
        tokenizer.encode("ab", FramingPolicy::ContentOnly),
        [2, 3, 4]
    );
    assert_eq!(
        tokenizer.encode("🙂🙂猫", FramingPolicy::ModelInput),
        [2, 0, 6, 1]
    );
    assert_eq!(tokenizer.encode("</s>", FramingPolicy::ContentOnly), [2, 0]);
    let mut no_prefix = bytes;
    no_prefix.extend(field(3, &[24, 0, 32, 0, 40, 0]));
    let tokenizer = T5Tokenizer::from_sentencepiece_model(&no_prefix).unwrap();
    assert_eq!(
        tokenizer.encode("a  b", FramingPolicy::ContentOnly),
        [3, 0, 4]
    );
}

#[test]
fn t5_rejects_corrupt_maps_empty_pieces_nonfinite_scores_and_wrong_models() {
    for suffix in [
        field(3, &field(2, &[0, 0, 0, 0])),
        piece("", -1.0, 1),
        piece("bad", f32::NAN, 1),
        field(2, &[24, 2]),
        piece("duplicate_unknown", 0.0, 2),
        piece("a", 0.0, 1),
        piece("invalid_type", 0.0, 99),
    ] {
        let mut bytes = t5_asset();
        bytes.extend(suffix);
        assert!(T5Tokenizer::from_sentencepiece_model(&bytes).is_err());
    }
    assert!(T5Tokenizer::from_sentencepiece_model(&[0]).is_err());
    assert!(T5Tokenizer::from_sentencepiece_model(&[255; 12]).is_err());
}
