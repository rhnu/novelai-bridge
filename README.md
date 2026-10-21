# novelai-bridge

A Rust SDK for NovelAI image generation. Build and validate requests locally, prepare image inputs,
and execute them through the included HTTP adapter or your own transport.

Supports Diffusion V3, V4, V4.5 and V5, including V5 Full Medium and Max Enhance. Model descriptors
provide defaults, supported operations, parameter constraints and pricing rules; V5 Full is the
default model.

## Quick start

Add the SDK and an async runtime:

```toml
[dependencies]
novelai-bridge = "0.7.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Set `NOVELAI_API_KEY` to your NovelAI persistent API token. This example sends a generation request
and saves every returned image:

```rust,no_run
use std::sync::Arc;

use novelai_bridge::{
    Client, ClientConfig, GenerateImageRequest, Model, ReqwestOptions, ReqwestTransport,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let transport = ReqwestTransport::new(ReqwestOptions::default())?;
    let client = Client::new(Arc::new(transport), ClientConfig::default());
    let request = GenerateImageRequest::builder("a cat sitting by a window", Model::NaiDiffusion5Full)
        .negative_prompt("blurry, low quality")
        .build();
    let result = client.generate(request).await?;

    for (index, image) in result.images.iter().enumerate() {
        let path = format!("image-{index}.{}", image.format().extension());
        std::fs::write(path, image.bytes())?;
    }
    Ok(())
}
```

`ReqwestOptions` configures credentials and timeout. `ClientConfig` configures service routing,
response budgets and image request encoding. Cloned clients share the transport, subscription
snapshot and image-cache state.

## Capabilities

| Area | Entry points and behavior |
| --- | --- |
| Generation | `GenerateImageRequest::builder` for text-to-image, image-to-image, inpainting and Max Enhance; character prompts, image references and transparent output where supported. |
| Streaming | `Client::generate_stream` for SSE or length-prefixed MessagePack previews and final images. |
| Image tools | `Client::run_director_tool` for Director operations, including dialogue-preserving declutter; `Client::upscale` for standalone upscaling. Both return every output image. |
| Vibe Transfer | Raw image references for V3, encoded references for V4/V4.5, and `Client::encode_vibe` with optional masks, cropping and explicit seeds. |
| Tag suggestions | `Client::suggest_tags` for English and Japanese queries, preserving provider order and language-specific ranking fields. |
| Local files | `VibeDocument` reads and writes official vibe files, bundles and PNG embeddings with bounded parsing and identity validation. |
| Metadata | `parse_image_metadata_from_bytes` reads PNG, JPEG and WebP metadata; `write_png_text_chunks` updates PNG text without re-encoding pixels. |
| Tokens and costs | Local CLIP, T5 and Qwen tokenization, Anlas estimates, subscription snapshots and runtime credit observation. |

Operation availability depends on the selected model. See [model descriptors](docs/model-descriptors.md)
for model-specific behavior and the [API reference](https://docs.rs/novelai-bridge) for request types.

## Validate and prepare locally

Requests default to `ValidationPolicy::Normalize`: supported adjustments produce diagnostics,
while invalid request shapes fail. Use `.policy(ValidationPolicy::Reject)` to reject settings
that would require adjustment. Unknown serialized request fields are rejected under either policy.

The generation path separates three steps:

1. `request.normalize()` resolves the effective model, parameters, conditioning text and geometry.
2. `plan.prepare()` processes image inputs and resolves an automatic seed without network access.
3. `client.generate_prepared(prepared).await` sends that fixed preparation.

Use `plan.diagnostics()` to explain adjustments and `plan.geometry()` to inspect input, expected
output and pricing dimensions. `prepared.encode(GenerationDelivery::Images)` exposes the wire JSON
without sending it. `client.generate(request)` performs the preparation steps for you.

Image operations take validated `EncodedImage` inputs. Inpainting starts with
`InpaintingInput::new(image, mask)` for pure infill; V4 and later can add source conditioning through
`input.img2img = Some(InpaintImageToImage::new(strength, noise))`. Max Enhance uses the builder's
`.max_enhance(input)` method and retains separate output and pricing geometry.

Returned `GeneratedImage` values expose bytes, format, metadata and source identity, including ZIP
entry names or JSON response positions. `GenerateImageResult::resolved_seed` is the request seed;
`image.seed()` reports an embedded sample seed when present.

## Consume a stream

Add `futures-util` to your dependencies. With the client from the quick start:

```rust,no_run
use futures_util::StreamExt;
use novelai_bridge::{GenerateImageStreamRequest, ImageStreamEvent};

let request = GenerateImageRequest::builder("a cat", Model::NaiDiffusion5Full).build();
let mut stream = client.generate_stream(GenerateImageStreamRequest {
    base: request,
    ..Default::default()
}).await?;

while let Some(item) = stream.next().await {
    let chunk = item?;
    match chunk.event {
        ImageStreamEvent::Intermediate { samp_ix, step_ix, .. } => {
            println!("sample {samp_ix}: preview at step {step_ix}");
        }
        ImageStreamEvent::Final { samp_ix, image, .. } => {
            println!("sample {samp_ix}: {} final image bytes", image.bytes().len());
        }
        _ => {}
    }
}
```

Each final event contains a validated image. Successful completion requires a final image for every
expected sample and EOF; previews alone, missing finals and provider errors cannot establish success.
Consume the stream to completion to detect terminal failures. `chunk.raw` retains the original SSE
or MessagePack source; select `StreamMode::MessagePack` for the binary protocol.

Call `stream.cancel().await` to stop consumption and finish any pending observer notification.
Dropping the stream cannot deliver an asynchronous notification.

## Cargo features

Request planning, media processing, metadata, the client, streaming and credit observation are part
of the core. Three features select adapters and tokenizer support:

| Feature | Provides | Default |
| --- | --- | --- |
| `reqwest-transport` | Authenticated reqwest HTTP transport | Yes |
| `tokenizer` | Local CLIP, T5 and Qwen encoders with caller-supplied assets | Yes |
| `bundled-assets` | Pinned embedded tokenizer assets; enables `tokenizer` | No |

For embedded assets, enable `bundled-assets` and load `Tokenizers::bundled()`. Attach tokenizers with
`Client::with_tokenizers` to check prompt lengths before sending. Loading and encoding never download
assets; without an attached registry, client-side token checks are skipped.

For a custom transport without tokenization:

```toml
novelai-bridge = { version = "0.7.0", default-features = false }
```

Implement `Transport::execute` to return status, headers and a raw byte stream.
`TransportResponse::from_bytes` adapts a complete body. The core applies response budgets, HTTP
error handling and protocol decoding to every transport.

## Integration options

- Choose JSON image responses with `.response_format(ImageResponseFormat::Json)`. ZIP and JSON
  responses share image-count and byte limits.
- Set `ClientConfig::image_request_encoding` to `ImageRequestEncoding::Multipart` for binary media
  parts. Enable `ClientConfig::image_cache` for bounded server-cache registration shared by client
  clones. An explicit invalid-cache-key rejection permits one recovery attempt with retained media;
  ambiguous transport failures are returned directly.
- Use `Client::get_subscription()` to refresh the account snapshot used for estimates, or
  `estimate_anlas_cost` with an explicit `AnlasPricingContext` for offline calculations.
  Estimates and sampled balances do not establish the actual charge for an individual request.
- Attach a `CreditObserver` with `Client::with_credit_observer` for estimates and terminal outcomes.
  Applications own persistence and logging; the library emits `tracing` events without installing
  a subscriber. Treat subscription `raw_response` as private account data.

## Documentation

- [API reference](https://docs.rs/novelai-bridge)
- [Models, supported operations and request options](docs/model-descriptors.md)
- [Tokenization and asset loading](docs/image-tokenizers.md)
- [Anlas pricing and subscription allowances](docs/anlas-pricing.md)
- [Development and verification](docs/development.md)
- [0.7.0 release notes and migration](CHANGELOG.md#070---2026-10-21)

This project is not affiliated with NovelAI. Licensed under [Apache-2.0](LICENSE).
See [credits](CREDITS.md) for third-party sources and acknowledgements.
