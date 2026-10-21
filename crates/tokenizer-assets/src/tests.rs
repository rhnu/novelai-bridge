use std::{fmt::Write as _, io::Read, string::String, vec::Vec};

use sha2::{Digest, Sha256};

use crate::{CLIP_MERGES_GZ, QWEN_TOKENIZER_CONFIG_GZ, QWEN_TOKENIZER_GZ, T5_SENTENCEPIECE_GZ};

#[test]
fn embedded_assets_match_pinned_upstream_hashes() {
    let sources: toml::Value = toml::from_str(include_str!("../assets/SOURCES.toml")).unwrap();
    let records = sources["asset"].as_array().unwrap();
    assert_eq!(records.len(), 4);
    for (name, bytes) in [
        ("clip", CLIP_MERGES_GZ),
        ("t5", T5_SENTENCEPIECE_GZ),
        ("qwen", QWEN_TOKENIZER_GZ),
        ("qwen-config", QWEN_TOKENIZER_CONFIG_GZ),
    ] {
        let record = records
            .iter()
            .find(|record| record["name"].as_str() == Some(name))
            .unwrap();
        let revision = record["revision"].as_str().unwrap();
        assert_eq!(revision.len(), 40);
        assert!(record["url"].as_str().unwrap().contains(revision));
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut decoded)
            .unwrap();
        assert_ne!(decoded.as_slice(), &[]);
        let upstream = if record.get("upstream_gzip").and_then(toml::Value::as_bool) == Some(true) {
            bytes
        } else {
            decoded.as_slice()
        };
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(upstream) {
            write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
        }
        assert_eq!(digest, record["sha256"].as_str().unwrap(), "{name}");
    }
}
