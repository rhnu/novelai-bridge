//! Exact upstream IDs for the bytes shipped in the optional asset package.

use std::io::Read;

use novelai_bridge::{
    FramingPolicy, ImageTokenizer, ImageTokenizerKind, TokenizerError, Tokenizers,
};
use novelai_bridge_tokenizer_assets::{
    CLIP_MERGES_GZ, QWEN_TOKENIZER_CONFIG_GZ, QWEN_TOKENIZER_GZ, T5_SENTENCEPIECE_GZ,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    source_url: String,
    revision: String,
    license: String,
    config_sha256: Option<String>,
    sha256: String,
    vocabulary_size: usize,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    input: String,
    content_only: Vec<u32>,
    model_input: Vec<u32>,
}

fn expand(bytes: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut decoded)
        .unwrap();
    decoded
}

fn read_fixture(name: &str) -> Fixture {
    let fixture = match name {
        "clip.json" => include_str!("../support/tokenizer/clip.json"),
        "t5.json" => include_str!("../support/tokenizer/t5.json"),
        "qwen.json" => include_str!("../support/tokenizer/qwen.json"),
        _ => panic!("unknown fixture {name}"),
    };
    serde_json::from_str(fixture).expect("fixture is valid JSON")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// Runs every fixture case against an encoder, asserting exact ID sequences under both policies.
fn check_fixture(encoder: &ImageTokenizer, fixture: &Fixture, asset: &[u8]) {
    assert_eq!(fixture.revision.len(), 40, "fixture must pin a commit");
    assert!(fixture.source_url.contains(&fixture.revision));
    assert!(matches!(fixture.license.as_str(), "MIT" | "Apache-2.0"));
    assert_eq!(
        sha256_hex(asset),
        fixture.sha256,
        "embedded asset does not match the revision the fixture was generated from"
    );
    assert_eq!(encoder.vocabulary_size(), fixture.vocabulary_size);

    for case in &fixture.cases {
        assert_eq!(
            encoder
                .encode(&case.input, FramingPolicy::ContentOnly)
                .unwrap(),
            case.content_only,
            "{}: content-only ids",
            case.name
        );
        assert_eq!(
            encoder
                .encode(&case.input, FramingPolicy::ModelInput)
                .unwrap(),
            case.model_input,
            "{}: model-input ids",
            case.name
        );
        assert_eq!(
            encoder
                .count(&case.input, FramingPolicy::ModelInput)
                .unwrap(),
            u32::try_from(case.model_input.len()).unwrap(),
            "{}: count matches sequence length",
            case.name
        );
    }
}

#[test]
fn clip_matches_upstream_golden_ids() {
    let asset = CLIP_MERGES_GZ;
    let tokenizers = Tokenizers::new().with_clip_merges(asset).unwrap();
    let encoder = tokenizers.get(ImageTokenizerKind::Clip).unwrap();
    check_fixture(encoder, &read_fixture("clip.json"), asset);
}

#[test]
fn t5_matches_upstream_golden_ids() {
    let asset = expand(T5_SENTENCEPIECE_GZ);
    let tokenizers = Tokenizers::new().with_t5_sentencepiece(&asset).unwrap();
    let encoder = tokenizers.get(ImageTokenizerKind::T5).unwrap();
    check_fixture(encoder, &read_fixture("t5.json"), &asset);
}

#[test]
fn qwen_matches_upstream_golden_ids() {
    let asset = expand(QWEN_TOKENIZER_GZ);
    let config = expand(QWEN_TOKENIZER_CONFIG_GZ);
    let tokenizers = Tokenizers::new()
        .with_qwen_json(&asset, Some(&config))
        .unwrap();
    let encoder = tokenizers.get(ImageTokenizerKind::Qwen).unwrap();
    let fixture = read_fixture("qwen.json");
    assert_eq!(
        fixture.config_sha256.as_deref(),
        Some(sha256_hex(&config).as_str())
    );
    check_fixture(encoder, &fixture, &asset);
}

#[test]
fn truncated_clip_asset_is_rejected_at_load_time() {
    let asset = CLIP_MERGES_GZ;
    let truncated = &asset[..asset.len() / 2];
    let error = Tokenizers::new().with_clip_merges(truncated).unwrap_err();
    assert!(
        matches!(
            error,
            TokenizerError::AssetDecompressionFailed { .. } | TokenizerError::MalformedAsset { .. }
        ),
        "{error}"
    );
}
