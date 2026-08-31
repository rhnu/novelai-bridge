# novelai-bridge-tokenizer-assets

Pinned, gzip-compressed CLIP, T5 and Qwen tokenizer assets. This `no_std` crate exposes four byte
slices and has no runtime dependencies, parsing or I/O.

| Export | Upstream asset |
| ------ | -------------- |
| `CLIP_MERGES_GZ` | OpenAI CLIP `bpe_simple_vocab_16e6.txt.gz` |
| `T5_SENTENCEPIECE_GZ` | google-t5/t5-base `spiece.model` |
| `QWEN_TOKENIZER_GZ` | Qwen/Qwen3.5-35B-A3B `tokenizer.json` |
| `QWEN_TOKENIZER_CONFIG_GZ` | Qwen/Qwen3.5-35B-A3B `tokenizer_config.json` |

CLIP preserves the upstream gzip bytes. T5 and Qwen preserve the upstream content inside a
deterministic gzip wrapper. Decompress them before passing them to a raw-format loader.

Most users should enable `novelai-bridge`'s `bundled-assets` feature and call `Tokenizers::bundled()`.
The bridge handles decompression, validation and shared encoder caching. This package's version
tracks the asset snapshot independently of the bridge; model limits and encoding algorithms do
not belong here.

## Sources and licenses

[SOURCES.toml](assets/SOURCES.toml) records exact URLs, revisions, upstream SHA-256 hashes and dates.
CLIP is copyright (c) 2021 OpenAI, under [MIT](assets/LICENSE-CLIP). T5 and Qwen are distributed
under [Apache-2.0](LICENSE). The Rust wrapper is Apache-2.0; redistribution of the combined package
must retain both licenses and these source notices. No NovelAI frontend code is included.

Repository maintenance: `cargo xtask fetch-tokenizer-assets` fetches and verifies these pinned
files explicitly. It never runs during normal builds or at runtime. Update the source manifest
and the bridge's golden fixtures together when changing an asset snapshot.
