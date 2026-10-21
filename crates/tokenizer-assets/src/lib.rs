//! Pinned gzip-compressed tokenizer assets for `novelai-bridge`.
//!
//! This crate only exposes bytes: no parsing, decompression, filesystem or network access.
//! Most callers should enable `novelai-bridge`'s `bundled-assets` feature and use
//! `Tokenizers::bundled()` instead of loading these constants directly.
//!
//! CLIP retains its upstream gzip stream. T5 and Qwen are losslessly gzip-wrapped.
//! See `assets/SOURCES.toml` in the package for revisions, upstream hashes and licenses.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests;

/// `OpenAI` CLIP's `bpe_simple_vocab_16e6.txt.gz` (MIT).
pub static CLIP_MERGES_GZ: &[u8] = include_bytes!("../assets/bpe_simple_vocab_16e6.txt.gz");

/// T5-base's `spiece.model`, gzip-compressed (Apache-2.0).
pub static T5_SENTENCEPIECE_GZ: &[u8] = include_bytes!("../assets/spiece.model.gz");

/// Qwen3.5-35B-A3B's `tokenizer.json`, gzip-compressed (Apache-2.0).
pub static QWEN_TOKENIZER_GZ: &[u8] = include_bytes!("../assets/tokenizer.json.gz");

/// Qwen3.5-35B-A3B's `tokenizer_config.json`, gzip-compressed (Apache-2.0).
pub static QWEN_TOKENIZER_CONFIG_GZ: &[u8] = include_bytes!("../assets/tokenizer_config.json.gz");
