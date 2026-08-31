//! Asset-independent tokenizer checks; bundled golden tests also run from packaged sources.

#![cfg(feature = "tokenizer")]

#[cfg(feature = "bundled-assets")]
#[path = "tokenizer/golden.rs"]
mod golden;

use novelai_bridge::{ImageTokenizerKind, Model, TokenizerError, Tokenizers};

#[test]
fn model_capabilities_select_the_documented_encoder_and_limit() {
    let expected = [
        (Model::NaiDiffusion3, ImageTokenizerKind::Clip, 225),
        (Model::NaiDiffusion3Furry, ImageTokenizerKind::Clip, 225),
        (Model::NaiDiffusion4Full, ImageTokenizerKind::T5, 512),
        (Model::NaiDiffusion4Curated, ImageTokenizerKind::T5, 512),
        (Model::NaiDiffusion45Full, ImageTokenizerKind::T5, 512),
        (Model::NaiDiffusion45Curated, ImageTokenizerKind::T5, 512),
        (Model::NaiDiffusion5Full, ImageTokenizerKind::Qwen, 1471),
        (Model::NaiDiffusion5Curated, ImageTokenizerKind::Qwen, 703),
    ];

    for (model, tokenizer, limit) in expected {
        let capabilities = model.capabilities();
        assert_eq!(capabilities.tokenizer, tokenizer, "{model:?}");
        assert_eq!(capabilities.prompt_token_limit, limit, "{model:?}");
    }
}

#[test]
fn missing_encoder_is_reported_rather_than_guessed() {
    let tokenizers = Tokenizers::new();
    assert!(tokenizers.is_empty());

    let error = tokenizers
        .require_for_model(Model::NaiDiffusion5Full)
        .unwrap_err();
    assert!(matches!(
        error,
        TokenizerError::TokenizerNotLoaded {
            kind: ImageTokenizerKind::Qwen
        }
    ));
}

#[test]
fn qwen_asset_with_a_dangling_merge_is_rejected() {
    // A merge whose result is absent from the vocabulary would silently produce wrong counts.
    let asset = serde_json::json!({
        "added_tokens": [],
        "normalizer": {"type": "NFC"},
        "model": {
            "type": "BPE",
            "vocab": {"a": 0, "b": 1},
            "merges": [["a", "b"]],
        }
    });

    let error = Tokenizers::new()
        .with_qwen_json(asset.to_string().as_bytes(), None)
        .unwrap_err();
    assert!(
        matches!(error, TokenizerError::MissingMergeSymbol { .. }),
        "{error}"
    );
}

#[test]
fn qwen_asset_with_duplicate_ids_is_rejected() {
    let asset = serde_json::json!({
        "model": {"type": "BPE", "vocab": {"a": 0, "b": 0}, "merges": []}
    });

    let error = Tokenizers::new()
        .with_qwen_json(asset.to_string().as_bytes(), None)
        .unwrap_err();
    assert!(
        matches!(error, TokenizerError::DuplicateTokenId { id: 0, .. }),
        "{error}"
    );
}

#[test]
fn qwen_special_token_id_collision_is_rejected() {
    let asset = serde_json::json!({
        "added_tokens": [{"id": 7, "content": "a"}],
        "model": {"type": "BPE", "vocab": {"a": 0}, "merges": []}
    });

    let error = Tokenizers::new()
        .with_qwen_json(asset.to_string().as_bytes(), None)
        .unwrap_err();
    assert!(
        matches!(error, TokenizerError::SpecialTokenIdCollision { id: 0, .. }),
        "{error}"
    );
}

#[test]
fn malformed_t5_asset_is_rejected() {
    let error = Tokenizers::new()
        .with_t5_sentencepiece(&[0xff, 0xff, 0xff])
        .unwrap_err();
    assert!(
        matches!(error, TokenizerError::MalformedAsset { .. }),
        "{error}"
    );
}
