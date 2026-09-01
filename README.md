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

The default model is `Model::NaiDiffusion5Full` (`nai-diffusion-5-full`), with guidance scale 7.

## Model descriptors

`Model::descriptor()` returns a read-only registry entry with separate identity, support,
constraints, defaults, prompt, sampling, wire and pricing profiles. Builders, normalization,
tokenization, pricing and sending all consume these profiles.

```rust
use novelai_bridge::{GenerationMode, Model, ReferenceIntent};

let descriptor = Model::NaiDiffusion45Full.descriptor();
assert_eq!(descriptor.prompt().token_limit, 512);
let references = descriptor.resolve_references(
    GenerationMode::ImageToImage,
    ReferenceIntent { vibe: true, character: true },
);
assert!(references.character.is_available());
assert!(!references.vibe.is_available());
```

Optional profiles bind capability and parameters together: an inpainting sibling supplies its
model ID, a character-reference profile supplies its allowed modes, and variety boost supplies
its sigma coefficient. V5 has no reference profiles. V3's provider-only legacy vibe path remains
inspectable but cannot be encoded or sent by this bridge.

`ModelCapabilities` and `capabilities()` have been removed. See
[model descriptors and migration](docs/model-descriptors.md) for replacements and design decisions.

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

Image-bearing request fields use `EncodedImage`, which validates bytes and caches decoded pixels
at construction. Use `EncodedImage::from_bytes` for binary input or `from_base64` at text/config
boundaries. Vibe data is deliberately separate: `Client::encode_vibe` returns `VibeEncoding`, and
`VibeReference::vibe_data_cache` accepts that type directly. `NormalizedGeneration::prepare_with`
can inject an `ImagePreprocessor`; the default preserves the existing source, mask and director
reference fitting behavior.

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

### Custom transports and streaming

`Transport` receives a fully encoded `TransportRequest` and returns status, headers and raw bytes
in `TransportResponse`. It does not serialize generic Rust values or interpret NovelAI errors.
Implement `StreamingTransport` only when SSE is needed; its events contain generic SSE `data`,
while the bridge decodes `ImageStreamChunk` above the transport boundary.

Streaming observations settle on actual EOF or stream error, not when the HTTP connection opens.
Call `GenerateImageStream::cancel().await` to record explicit cancellation. Dropping a stream cannot
run an async observer and deliberately emits no guessed terminal result.

### Effective generation plans

`GenerateImageRequest` remains editable input. `normalize()` follows its `strict_mode`;
`normalize_with(NormalizationPolicy::Reject | Normalize)` overrides that policy explicitly.
The resulting `NormalizedGeneration` has immutable effective fields, cached conditioning text,
and structured diagnostics describing clamps, fallbacks, conflicts and dropped features.

Normalize once when a caller needs both prompt inspection and pricing:

```rust
use novelai_bridge::{AnlasPricingContext, GenerateImageRequest, Model};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Curated).build();
let plan = request.normalize()?;
let context = AnlasPricingContext::default();
let estimate = plan.estimate_anlas_cost(context);
println!("{} Anlas; {:?}", estimate.total, plan.diagnostics());
println!("conditioning: {}", plan.resolved_prompts().prompt);
let prepared = plan.prepare()?;
// client.generate_prepared(prepared).await?;
// Or: client.generate_stream_prepared(prepared, StreamMode::default()).await?;
# Ok(())
# }
```

Image payloads are already validated and decoded when `EncodedImage` is constructed.
Normalization and estimates do not repeat that work, resolve random seeds or access the network.
`prepare()` fits cached pixels and resolves the seed once; the client can send that
`PreparedGeneration` without repeating these steps. Normal `generate` and `generate_stream`
perform preparation automatically.

Ordinary generation and director responses accept only validated PNG, JPEG or WebP bytes, either
raw or in a bounded ZIP archive. A malformed image candidate rejects the whole response;
best-effort metadata parsing remains represented on an otherwise valid `GeneratedImage`.

Disabled characters do not participate in validation, counting or wire arrays. Normalize policy
keeps the first model-limit enabled characters and disables the rest, preserving original indices;
Reject policy errors on overflow. Coordinates use the existing five-by-five wire grid, including
in Reject mode. Native noise scheduling on V4+ retains the existing Karras fallback and reports it.

Raw-request `resolve_prompts`, `count_tokens`, `anlas_estimate_input` and `estimate_anlas_cost`
now return `Result` because they normalize first. Repeated calls clone the raw input; reuse a plan
to avoid copying image payloads and rebuilding prompts. Normalized token counting still returns
`TokenizerError`. The client estimate uses its most recently sampled account context; estimates
agree for the same effective request and context, but later account samples may change pricing.

`estimate_anlas_cost(AnlasEstimateInput { .. })` is the independent mathematical entry point:
it does not normalize or guarantee that its input is sendable. The calculator
implements the NAI 3, 4, 4.5, 5 branch only; older and unrelated model price tables are intentionally out
of scope. Check `AnlasEstimate::is_too_expensive()` for the web client's over-140 state.

Disclaimer:

- This project is under experimental development and does not guarantee API or behavior stability.
- Development of this project has been assisted by Codex.

Notes:

- `Model::descriptor()` describes static model knowledge; `resolve_references(mode, intent)`
  answers request-specific reference availability using the same rules as normalization.
- Request fields are grouped by capability. A group the selected model does not support is
  rejected in `strict_mode` and dropped with a normalization diagnostic otherwise. Inspect a plan
  before sending when these decisions need to be displayed; they are no longer only log warnings.
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
