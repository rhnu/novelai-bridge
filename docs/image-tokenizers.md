# Image prompt tokenization

This document describes local token encoding and counting in `novelai-bridge`. Counts use the
selected upstream tokenizer assets; they do not model NovelAI web-editor preprocessing such as
prompt weights or multiprompt syntax.

## Model selection

`Model::capabilities()` supplies both the encoder kind and the limit for each conditioning field.
These capabilities remain available without the `tokenizer` feature.

| Model family | Encoder | Tokens per field |
| ------------ | ------- | ---------------: |
| NAI 3 / Furry 3 | CLIP | 225 |
| NAI 4 / 4.5 | T5 | 512 |
| NAI 5 Full | Qwen | 1471 |
| NAI 5 Curated | Qwen | 703 |

Inpainting uses the base model's capabilities. The limit is neither the vocabulary size nor a
combined budget for positive, negative and character prompts.

## Encoding

`ImageTokenizer::encode(text, framing)` returns ordered token IDs. `count` returns the length of
the same sequence. Both return errors rather than a partial result when encoding fails.

### CLIP

The encoder applies NFC normalization, two HTML entity-unescape passes, whitespace collapsing and
lowercasing. CLIP's regex splits the text before byte-level BPE; vocabulary IDs follow the
upstream byte-alphabet order, and word-final pieces carry `</w>`.

Full `ftfy` repair is not implemented. Mojibake, quotes, character widths, ligatures and control
characters can therefore differ from the OpenAI Python reference.

### T5

The encoder reads pieces, scores and trainer/normalizer settings from a SentencePiece Unigram
model. Its embedded character map performs normalization, followed by the model's dummy-prefix,
whitespace and escaping rules.

Viterbi search selects the highest-scoring segmentation using `f32` scores and lower token IDs
to break ties. Unknown pieces use the lowest normal-piece score minus 10; adjacent unknowns are
merged. User-defined pieces are protected, while control pieces are not matched from input.

Unsupported profiles, including byte fallback, are rejected. The bundled T5 model does not add
Hugging Face's `<extra_id_*>` vocabulary extension.

### Qwen

The encoder reads ranked byte-level BPE and added tokens from `tokenizer.json`, merging any
additional tokens from `tokenizer_config.json`. Token text and IDs must agree between the files.

Added tokens are recognized before normalizing ordinary text. The supported profile uses null or
NFC normalization, isolated regex splitting and ByteLevel encoding without an implicit prefix
space. Unregistered token-like strings remain ordinary text. Unsupported settings, including
dropout and automatic truncation or padding, are rejected.

Both BPE encoders use bounded word caches. T5 uses indexed piece lookup bounded by the longest
piece rather than greedy longest-match segmentation.

## Framing

| Policy | CLIP | T5 | Qwen |
| ------ | ---- | -- | ---- |
| `ContentOnly` | Content IDs | Content IDs | Content IDs |
| `ModelInput` | SOT + content + EOT | Content + EOS | Content IDs |

Request counting defaults to `ModelInput`; `count_tokens_with` accepts an explicit policy.

## Conditioning text and limits

`GenerateImageRequest::resolve_prompts()` assembles the text used by both wire conditioning and
token counting:

1. Positive prompt: caller text, supported transparency tag, then quality tags.
2. Negative prompt: UC preset followed by caller text.
3. Characters: supported, enabled characters only, without quality or UC presets. Each keeps its
   index in the original character list.

The top-level wire `input` retains the caller's original prompt; conditioning uses the resolved
legacy prompt/UC fields or V4/V5 captions.

Request and stream-request `count_tokens` return independent `used` / `limit` values for each
field. `PromptTokenUsage::over_limit` reports fields where `used > limit`; counts are not summed.

An optional `Client::with_tokenizers(Arc<Tokenizers>)` enables checks after request normalization
and before image-request construction or transport. Strict requests return
`InvalidRequest::PromptTokenLimitExceeded`; non-strict requests log a warning without truncation.
Missing encoders and encoding failures return `BridgeError::Tokenizer` in either mode.

Without an injected registry, generation skips token checks. Explicit `Client::count_tokens`
still reports a missing encoder and never performs network I/O.

## Asset loading

The default `tokenizer` feature enables encoders but does not load assets. `Tokenizers` accepts
caller-provided bytes through `with_clip_merges` (gzip or text), `with_t5_sentencepiece` (raw model)
and `with_qwen_json` (raw JSON plus optional configuration).

The optional `bundled-assets` feature uses `novelai-bridge-tokenizer-assets`: pinned OpenAI CLIP,
google-t5/t5-base and Qwen/Qwen3.5-35B-A3B files. `Tokenizers::bundled()` decompresses and validates
them on first use, caches the parsed encoders globally, and returns registries sharing them via
`Arc`. Neither loading path downloads files or manages a disk cache.
