# novelai-bridge

`novelai-bridge` is a Rust library for NovelAI image-generation APIs and a small set of related helpers.

Scope:

- `generate` (text-to-image, image-to-image and inpainting)
- `generate_stream`
- `encode_vibe`
- `augment-image` director tools
- `get_subscription`
- model capability descriptions
- optional credit observation and NAI 3, 4, 4.5, 5 Anlas cost estimation
- PNG metadata parsing
- embedded vibe document extraction
- local image payload normalization / reference preprocessing helpers

Supported models: NovelAI Diffusion V5, V4.5, V4, V3 and Furry V3.

## Local prompt token counts

The default `tokenizer` feature provides CLIP, T5 and Qwen encoders. Disable default features to
omit encoder dependencies; model capabilities and prompt limits remain available.

Load your own assets with `Tokenizers::with_clip_merges`, `with_t5_sentencepiece` or
`with_qwen_json`, or enable the optional asset package:

```toml
novelai-bridge = { version = "0.5.0-rc.1", features = ["bundled-assets"] }
```

```rust,no_run
use novelai_bridge::{GenerateImageRequest, Model, Tokenizers};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let tokenizers = Tokenizers::bundled()?;
let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full).build();
let usage = request.count_tokens(&tokenizers)?;
println!("{} of {}", usage.prompt.used, usage.prompt.limit);
# Ok(())
# }
```

Counting includes resolved quality/UC presets and enabled characters, with an independent limit
for each field. Inject an `Arc<Tokenizers>` through `Client::with_tokenizers` to check generation
requests locally: strict requests reject overflow; non-strict requests warn without truncation.
Without injection, generation does not check token limits.

The asset package is versioned separately; `Tokenizers::bundled()` shares parsed encoders across
calls. Neither asset-loading path performs network I/O. CLIP implements NFC and HTML cleanup but
not full `ftfy` text repair. See [image prompt tokenization](docs/image-tokenizers.md) for encoding,
framing and loading behavior.

## Image generation

```rust
use novelai_bridge::{ApiKeySource, BaseUrls, Client, ClientOptions, GenerateImageRequest, Model};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(ClientOptions {
        api_key_source: ApiKeySource::Environment,
        base_urls: BaseUrls::default(),
        ..Default::default()
    })?;

    let result = client
        .generate(GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full).build())
        .await?;

    println!(
        "generated {} image(s) with request seed {}",
        result.images.len(),
        result.resolved_seed
    );
    for image in &result.images {
        println!("embedded output seed: {:?}", image.seed());
    }
    Ok(())
}
```

When using `ApiKeySource::Environment`, set `NOVELAI_API_KEY` before constructing the client.

Cost estimates are available both as a standalone calculator and from an assembled request. They
share the same input mapping and formula:

```rust
use novelai_bridge::{AnlasPricingContext, GenerateImageRequest, Model};

let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Curated).build();
let estimate = request.estimate_anlas_cost(AnlasPricingContext::default());
let client_estimate = client.estimate_anlas_cost(&request);
assert_eq!(estimate, client_estimate);
```

`estimate_anlas_cost(AnlasEstimateInput { .. })` is the independent entry point. The calculator
implements the NAI 3, 4, 4.5, 5 branch only; older and unrelated model price tables are intentionally out
of scope. Check `AnlasEstimate::is_too_expensive()` for the web client's over-140 state.

Disclaimer:

- This project is under experimental development and does not guarantee API or behavior stability.
- Development of this project has been assisted by Codex.

Notes:

- `Model::capabilities()` describes what a model family supports; request assembly and validation
  are driven entirely by it, and callers can use it to gate their own UI.
- Request fields are grouped by capability. A group the selected model does not support is
  rejected in `strict_mode` and dropped with a `log::warn!` otherwise.
- Image streaming is supported by V4, V4.5, and V5. `Client::generate_stream` always rejects
  V3 and Furry V3 locally; it does not silently downgrade to a non-streaming request.
- `QualityPreset::Light` and `transparent_background` are V5-only; on earlier models they fall
  back to the standard behaviour, or are rejected in `strict_mode`.
- Image-to-image sources and inpaint masks are fitted to the requested canvas before sending.
- `GenerateImageResult::resolved_seed` is the request-level seed actually sent to NovelAI.
- Per-image seeds come only from embedded PNG metadata via `GeneratedImage::seed`; the request
  seed is never presented as an image seed.
- `inspect_generated_image_metadata` provides a non-failing metadata inspection path for complete
  generated images, while `parse_png_metadata_from_bytes` remains the strict PNG parser.
- Attaching a `CreditObserver` reports de-identified request fingerprints, local Anlas estimates
  and sampled balances; the crate itself persists nothing.
- The default transport uses `reqwest + rustls`.
- This project is not affiliated with NovelAI.

Credits: see [CREDITS.md](CREDITS.md).

License: [Apache-2.0](LICENSE)
