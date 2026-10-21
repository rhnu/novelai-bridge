# novelai-bridge

A Rust library for NovelAI image generation, director tools, vibe encoding, model descriptors, local tokenization, PNG metadata and Anlas estimates.

The default model is NovelAI Diffusion V5 Full. V5 Curated, V4.5, V4, V3 and Furry V3 are also supported. This project is experimental and RC APIs may change.

## Quick start

Set `NOVELAI_API_KEY` before creating the default HTTP transport:

```rust,no_run
use std::sync::Arc;
use novelai_bridge::{
    Client, ClientConfig, GenerateImageRequest, Model, ReqwestOptions, ReqwestTransport,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = ReqwestTransport::new(ReqwestOptions::default())?;
    let client = Client::new(Arc::new(transport), ClientConfig::default());
    let result = client
        .generate(GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full).build())
        .await?;
    println!("{} images; request seed {}", result.images.len(), result.resolved_seed);
    Ok(())
}
```

`Client` accepts a shared transport, including `Arc<dyn Transport>`. Cloning a client shares the transport and sampled account state; the transport itself does not need to implement `Clone`. HTTP credentials and timeout belong to `ReqwestOptions`, while service routing belongs to `ClientConfig`.

## Cargo features

Defaults enable the client, reqwest ordinary/SSE transport, credit observation and local tokenizers. Assets are loaded separately.

| Feature | Capability |
| --- | --- |
| No features | Model descriptors, common value types and pure Anlas pricing |
| `generation` | Request planning, image preparation and offline payload encoding |
| `client` | Client execution with an injected ordinary `Transport` |
| `reqwest-transport` | Authenticated reqwest/rustls HTTP adapter |
| `streaming` | Optional `StreamingTransport`, generation stream and lifecycle |
| `reqwest-streaming` | Reqwest SSE framing |
| `png-metadata` | Strict PNG metadata parsing without the image crate or ZIP |
| `image-processing` | Validated image inputs, preprocessing and raw/ZIP response decoding |
| `credit-observer` | Request lifecycle and sampled-account notifications |
| `tokenizer` | Independent CLIP, T5 and Qwen encoding |
| `bundled-assets` | Pinned, independently versioned tokenizer assets |

Tokenizer-only consumers do not compile generation, pixel processing or ZIP. Request-level token usage is available when both `generation` and `tokenizer` are enabled. Features never disable validation inside an API that remains available.

## Prepare once

Editable input becomes an immutable effective plan. Reject policy rejects invalid input; Normalize policy clamps or removes unsupported fields and records typed diagnostics.

```rust
use novelai_bridge::{
    AnlasPricingContext, GenerateImageRequest, GenerationDelivery, Model,
};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let plan = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
    .build()
    .normalize()?;
let estimate = plan.estimate_anlas_cost(AnlasPricingContext::default());
println!("{} Anlas; {:?}", estimate.total, plan.diagnostics());
let prepared = plan.prepare()?;
let body = prepared.encode(GenerationDelivery::Images)?;
println!("{} bytes; seed {}", body.len(), prepared.resolved_seed());
# Ok(())
# }
```

Normalization resolves conditioning text, enabled characters, coordinates and reference decisions once. Preparation transforms cached image pixels and resolves an automatic seed from the system random source. Encoding exposes read-only bytes for offline inspection or custom senders. `GenerationDelivery::Stream(mode)` selects streaming wire encoding and checks model support.

`Client::generate_prepared` and `generate_stream_prepared` reuse this result. Director and vibe requests likewise expose `prepare()` and their prepared values expose `encode()`, without constructing a client.

## Images and references

`EncodedImage::from_bytes` and `from_base64` validate image bytes and cache decoded pixels. Custom `ImagePreprocessor` implementations can borrow `EncodedImage::pixels()`; operations carry their target canvas, and preparation rejects an output whose dimensions differ.

`DirectorCommand` binds each operation to its valid parameters. Opaque `VibeEncoding` is distinct from image input. Generation reference support is resolved through the model descriptor for the actual request mode.

Generated images accept valid PNG, JPEG or WebP, raw or inside a bounded ZIP. Invalid image candidates reject the response. Metadata failures on an otherwise valid image remain visible through `GeneratedImageMetadata`. Request seeds and embedded per-image seeds are distinct.

## Local tokenization

Load caller-provided assets with `Tokenizers::with_clip_merges`, `with_t5_sentencepiece` or `with_qwen_json`, or enable `bundled-assets` and call `Tokenizers::bundled()`. No loading or encoding path downloads assets.

`TokenEncoder` is the encoding capability. `Tokenizers::register(kind, Arc<dyn TokenEncoder>)` accepts an already loaded implementation; built-in encoders implement the same interface. Model descriptors select the encoder kind and field limit.

Attach the registry with `Client::with_tokenizers` to check effective conditioning before sending. Strict requests reject overflow; non-strict requests warn without truncating. Without injection, sending skips local token checks. See [tokenization](docs/image-tokenizers.md).

## Accounts and observation

`get_subscription()` updates the client's sampled `SubscriptionInfo`, available through `last_subscription()`. Client estimates use that snapshot; pure estimates accept an explicit `AnlasPricingContext`. Invalid subscription responses do not overwrite a valid snapshot.

The optional `CreditObserver` receives operation-specific summaries, estimates and terminal outcomes, plus sampled subscriptions. The library does not persist these events.

Streaming settles on actual EOF, failure or explicit `GenerateImageStream::cancel().await`. Its successful count represents distinct samples received. Dropping a stream cannot run asynchronous callbacks and does not invent a terminal result.

## Further reading

- [Model descriptors and request availability](docs/model-descriptors.md)
- [Tokenization, framing and asset loading](docs/image-tokenizers.md)
- [Anlas estimate formula](docs/anlas-pricing.md)
- [Development tools and checks](docs/development.md)
- [Changelog](CHANGELOG.md) and [credits](CREDITS.md)

This project is not affiliated with NovelAI. Development has been assisted by Codex.

License: [Apache-2.0](LICENSE).
