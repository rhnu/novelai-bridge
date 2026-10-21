# novelai-bridge

A Rust client for NovelAI image generation, including streaming, image-to-image, inpainting,
Director Tools and Vibe Transfer. It also provides local tokenization, PNG metadata parsing and
Anlas cost estimates.

Supports the Diffusion V3, V4, V4.5 and V5 families. The default model is V5 Full;
[model descriptors](docs/model-descriptors.md) describe which operations each model supports.

## Get started

Add the client and an async runtime:

```toml
[dependencies]
novelai-bridge = "0.6.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Set `NOVELAI_API_KEY` to your NovelAI persistent API token, then generate and save an image:

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

Configure credentials and timeout through `ReqwestOptions`. Use `ClientConfig`
to change the service URL and response limits, or supply your own `Transport` implementation. Cloned clients share
the transport and cached subscription state.

## Stream images

Add `futures-util` to consume the stream. With the same client as above:

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

Each `Final` contains a complete, validated image. A multi-image request continues until every
expected sample has a final image and the stream reaches EOF. Empty or incomplete streams and
provider errors return `BridgeError::Stream`; a preview alone never establishes success.

Unknown extensions remain available as `ImageStreamEvent::Unknown`. Every chunk retains its
complete SSE name, ID, retry value and data in `chunk.raw`. Call `stream.cancel().await` to stop
consumption and finish any pending observer notification; dropping a stream cannot run that
asynchronous notification.

## Build and prepare requests

The builder exposes prompts, sampling, image size, references and output options. Image-based
operations use validated `EncodedImage` inputs:

| Operation      | Entry point                                                       |
| -------------- | ----------------------------------------------------------------- |
| Text-to-image  | `GenerateImageRequest::builder(...)`                              |
| Image-to-image | `.image_to_image(ImageToImageInput::new(...))`                    |
| Inpainting     | `.inpainting(InpaintingInput::new(image, mask))`                  |
| Director Tools | `Client::run_director_tool` with `RunDirectorToolRequest`         |
| Vibe encoding  | `Client::encode_vibe` with `EncodeVibeRequest`                    |
| Upscale        | `Client::upscale` with `UpscaleImageRequest::new(image)`          |
| Tag suggestions | `Client::suggest_tags` with `SuggestTagsRequest::new(prefix, model)` |

Requests default to `ValidationPolicy::Normalize`, which adjusts unsupported or out-of-range
settings and records diagnostics. Select `.policy(ValidationPolicy::Reject)` to reject those
settings instead. Unknown serialized request fields are rejected under both policies.

Inpainting defaults to pure infill. Set `input.img2img` to
`Some(InpaintImageToImage::new(strength, noise))` for additional source-image conditioning on
V4 and later. That group owns its color correction and optional noise seed; V3 supports pure
infill only.

For inspection or reuse, call `request.normalize()` to get the effective plan and its diagnostics,
then `plan.prepare()` to transform images and resolve an automatic seed. Send that preparation
with `client.generate_prepared(...)`, or inspect its JSON with
`prepared.encode(GenerationDelivery::Images)`. Preparing and encoding do not contact NovelAI.

Director calls return `RunDirectorToolResult` with every returned image. Returned `GeneratedImage`
values expose validated bytes, format, source identity, metadata and any embedded image seed. `GenerateImageResult::resolved_seed` is the request seed; it is separate from per-image seeds.

Director preparation resizes the source to the official tool's processing canvas. Inspect
`PreparedDirector::source_size()` and `image_size()` for the original and effective dimensions.
`DirectorCommand::DeclutterKeepBubbles` preserves dialogue bubbles during cleanup.

Standalone upscaling sends the original image and a typed blur declaration, with V5 Curated and
zero blur as defaults. It returns all images through `UpscaleImageResult::images`. Tag queries
support English and Japanese and return suggestions in provider order without generating images.
`SuggestTagsResult` retains English tag/count/confidence records or Japanese/English translation
pairs and their `power` ranking as separate variants.

## Tokenization and cost estimates

Tokenization runs locally. Enable `bundled-assets` and load `Tokenizers::bundled()`, or provide
CLIP, T5 or Qwen assets yourself. Attach the registry with `Client::with_tokenizers` to check prompt
lengths before sending. Without a registry, local token checks are skipped. Loading and encoding
never download assets. See [tokenization](docs/image-tokenizers.md) for supported formats and limits.

`Client::get_subscription()` refreshes the subscription snapshot used by client cost estimates.
It includes V5 regular and purchased allowance; `raw_response` retains the full account response,
including extensions. Treat that JSON as private account data. For offline estimates, use
`estimate_anlas_cost` with an explicit `AnlasPricingContext`. Estimates are local calculations;
see [Anlas pricing](docs/anlas-pricing.md) for the formula and allowance rules.

The optional `CreditObserver` receives request estimates, terminal outcomes and sampled
subscriptions. Applications own persistence and logging; the library emits `tracing` events
without installing a subscriber.

## Choose Cargo features

The core always includes request planning, media processing, metadata parsing, the client,
streaming and optional runtime credit observation. Cargo features select external adapters and
local tokenizer dependencies:

| Feature | Provides |
| --- | --- |
| No features | Complete core with an injected `Transport` |
| `reqwest-transport` | Authenticated reqwest HTTP byte transport |
| `tokenizer` | Local CLIP, T5 and Qwen encoders |
| `bundled-assets` | Pinned tokenizer assets; enables `tokenizer` |

Defaults enable `reqwest-transport` and `tokenizer`. Embedded assets remain opt-in. For a custom
transport without local tokenization, use:

```toml
novelai-bridge = { version = "0.6.0", default-features = false }
```

Response byte limits belong to `ClientConfig` and apply to custom transports as well as reqwest.
A transport returns response headers and a raw byte stream; the core handles bounded buffering,
HTTP errors and SSE decoding. `TransportResponse::from_bytes` adapts a complete response body.

## Documentation

- [API reference](https://docs.rs/novelai-bridge)
- [Models, supported operations and request options](docs/model-descriptors.md)
- [Tokenization and asset loading](docs/image-tokenizers.md)
- [Anlas pricing and subscription allowances](docs/anlas-pricing.md)
- [Development and verification](docs/development.md)
- [Release notes and migration](CHANGELOG.md)

This project is not affiliated with NovelAI. Licensed under [Apache-2.0](LICENSE).
See [credits](CREDITS.md) for third-party sources and acknowledgements.
