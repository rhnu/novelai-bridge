# Changelog

## Unreleased

### Changed

- Model generation input as distinct text-to-image, image-to-image and inpainting variants. Keep resolved model identity, conditioning, pricing and wire preparation on one immutable effective plan.
- Compose wire requests from required common fields and mode/protocol components; retain reference records until final serialization.
- Share typed request fields and `ValidationPolicy` across generation, director and vibe preparation. Preserve local preparation failures separately from invalid input.
- Share complete PNG validation and media resource policies across inputs, metadata, custom preprocessors and responses. Expose generated images through immutable getters and a common format mapping.
- Complete prepared Director/Vibe client entry points and centralize HTTP method, headers and routing in `ApiEndpoint`.
- Use structured tracing with stable request context and safe Debug summaries. The library emits events; xtask owns its subscriber and application error context.

### Fixed

- Bound actual HTTP body reads, retained error prefixes and raw SSE frames before their buffers grow. Mark truncated API error bodies explicitly.
- Reject incomplete or corrupt PNG structures instead of accepting them through a less strict pixel-decoding fallback. Bound decoded pixels and text as well as encoded bytes.
- Retain pending stream terminal callbacks when a polling future is cancelled; explicit cancellation completes the existing settlement exactly once.
- Correct T5 overlapping user-defined symbol scoring to match SentencePiece 0.2.2. Preserve tokenizer parser and regex error causes, including companion-config identity.
- Reject non-finite or unrepresentable metadata numbers with field diagnostics and retain source indices when pairing character prompts.
- Require the exact final base64 marker in image data URLs. Reject body edits on bodyless inspection operations.
- Remove unreachable canvas-area shrinking, ineffective pricing sample branches and tests that did not exercise their stated behavior.

### Migration

- Replace `Img2ImgRequest` and `.img2img(...)` with `ImageToImageInput` and `.image_to_image(...)`. For masked requests use `InpaintingInput::new(image, mask, strength, noise)` and `.inpainting(...)`; `GenerateImageRequest::operation` stores the selected `GenerationInput`. Inpainting does not accept color correction or an extra noise seed.
- Replace `strict_mode` and `NormalizationPolicy` with `policy: ValidationPolicy` or `.policy(...)`. Unsupported noise schedules now obey Reject/Normalize consistently. Generation request deserialization rejects unknown fields, including removed request fields.
- Match `RequestField` in validation errors instead of comparing path strings. Token diagnostics use the same editable locations. Custom `ImagePreprocessor::prepare` now receives `RequestField` by value.
- Handle `BridgeError::Preparation` for random-source, image-encoding and custom-preprocessor dimension failures. `TokenizerError::EncodingFailed` carries `source` instead of `reason`.
- Read generated images with `bytes()`, `format()`, `mime_type()` and `metadata()`, or consume `into_bytes()`. Generated-image results no longer deserialize unchecked bytes and metadata; decode media to recreate validated results.
- Use `PngMetadata::characters` records with `source_index`, `prompt` and `negative_prompt` instead of parallel character arrays. Match the diagnostic fields carried by `PngMetadataWarning::InvalidCommentJson` and numeric warnings.
- Construct `ApiError` from its status using `new(status, endpoint)` or `from_response_parts`; read `status()` and `kind()`. Custom transports provide `TransportRequest::content_type` and `TransportResponse::body_truncated`; use `ApiEndpoint::request` for built-in operations. Configure adapter budgets with `ReqwestOptions::response_limits`.
- Match `ImageStreamEventType` or use `as_str()` when inspecting stream event names; unknown names remain available. Use `run_director_tool_prepared` and `encode_vibe_prepared` after inspecting those prepared payloads.

## 0.5.0-rc.2 - 2026-09-04

### Added

- Add `GenerationCore::furry_mode` and the matching builder method for V4, V4.5 and V5. Anime
  remains the default; enabling furry mode prepends `fur dataset` to the effective positive prompt
  used by request encoding and token counting. V3 follows the existing strict/normalize handling
  for unsupported prompt features.
- Add `CharacterPositionProfile` to character prompt descriptors. V4/V4.5 explicitly select the
  discrete 5x5 grid, while V5 explicitly selects normalized freeform coordinates.

### Fixed

- Correct the maximum enabled character count for V5 Full and V5 Curated from 6 to 32.
- Preserve V5 freeform character coordinates through normalization and structured prompt wire
  assembly instead of applying the V4/V4.5 5x5 projection to every structured model.
- Separate Image2Image and Inpaint strength constraints. Image2Image remains `0.01..=0.99`, while
  masked Inpaint requests now accept and serialize strength `1.0` without normalization loss.

## 0.5.0-rc.1 - 2026-09-01

Release candidate during the tokenizer refactor; not a stable 0.5.0 release.

### Added

- Composable `generation`, `client`, `reqwest-transport`, `streaming`, `reqwest-streaming`,
  `png-metadata`, `image-processing` and `credit-observer` Cargo features. The no-default core now
  exposes model descriptors, common parameters and pure pricing without HTTP/image/SSE dependencies.
- `DirectorCommand`, whose variants carry only the parameters accepted by that director operation.
- `GenerateImageStreamRequest::base()` and its `AsRef<GenerateImageRequest>` implementation.
- `EncodedImage`, `VibeEncoding` and the replaceable `ImagePreprocessor` boundary. Validated image
  bytes retain cached decoded pixels through normalization and preparation.
- Strict, bounded raw/ZIP generated-image decoding with best-effort metadata state.
- Dyn-compatible fixed HTTP request/response DTOs and a separate `StreamingTransport` capability.
- `GenerateImageStream::cancel()` and `RequestOutcome::Cancelled` for explicit stream cancellation.
- Read-only `ModelDescriptor` profiles for identity, support, constraints, defaults, prompt,
  sampling, wire and pricing. Registry entries explicitly compose profiles without model inheritance.
- `GenerationMode`, `ReferenceIntent` and shared `resolve_references` decisions for model/mode
  support, unimplemented input paths and reference conflicts.
- Immutable `NormalizedGeneration`, explicit `NormalizationPolicy` and structured normalization
  diagnostics. Planning is independent of transport, image pixel decoding and seed resolution.
- `PreparedGeneration` and prepared ordinary/streaming send methods reuse validated input and a
  seed resolved once during image preparation.
- Local CLIP, T5 and Qwen encoders behind the default `tokenizer` feature, with token IDs,
  explicit framing, model selection and per-field counts on requests, stream requests and clients.
- Optional `Client::with_tokenizers` injection: strict requests reject overflow before transport;
  non-strict requests warn and preserve the full text.
- `bundled-assets` backed by the independently versioned `novelai-bridge-tokenizer-assets` 0.1.0
  package. Git and registry consumers use the same `Tokenizers::bundled()` entry point and shared
  encoder cache; callers can still supply their own asset bytes.
- Pinned asset provenance and licenses, SHA-256 checks, and reproducible upstream token-ID fixtures.

### Changed

- Split pure pricing from optional credit observation, move common model/request parameters into a
  lightweight always-available module, and make reqwest, SSE, image, ZIP and PNG stacks optional.
- Disable default features on `image`, `zip` and `reqwest`; retain only PNG/JPEG/WebP decoding,
  deflate ZIP responses and rustls/stream transport support required by the public contract.
- Box implementation-specific base64/image/PNG error sources so public errors do not bind optional
  dependency types while preserving source chains and structured field context.
- Move private generation/director wire types and helper logic into their owning feature modules;
  the former horizontal `types` and `internal` implementation modules are gone.
- Use semantic image and vibe payload types in request structs. Generation and director responses
  reject arbitrary bytes and reject a ZIP response when any supported-image candidate is invalid.
- Move JSON serialization, NovelAI status interpretation and stream-chunk decoding above transport;
  transports now expose raw response data and generic framed SSE events.
- Settle streaming credit observations only on EOF, stream failure or explicit cancellation. A
  dropped, unconsumed stream does not emit an invented terminal state.
- Change the default model from V4.5 Full to V5 Full; default sampling now uses guidance scale 7.
- Remove `ModelCapabilities` and `capabilities()` in favor of registry-owned descriptors.
  Optional reference profiles eliminate V5's contradictory reference-inpainting flag.
- Bind inpainting support to its sibling ID and expose `inpainting_id()` as `Option<&str>`.
  Distinguish V3's provider-only legacy vibe path from implemented encoded-vibe input.
- Move noise-schedule support/fallback to sampling, and derive protocol version/extensions from
  `WireProfile`. Default values and validation bounds now come from descriptor profiles.
- Normalize once and share cached conditioning text between wire construction, token counting
  and observation, including quality, transparency and UC presets.
- Raw request prompt, token and cost helpers are fallible and use the same effective fields as
  sending. Normalized cost projections no longer repeat capability and reference conflict rules.
- Skip disabled character validation; non-strict overflow disables characters after the model's
  enabled-character limit while preserving original indices. Strict overflow remains an error.
- Mark error enums, descriptor profile structs, `PromptStructure` and new tokenizer public types
  `#[non_exhaustive]`. UI selection enums and request structs retain exhaustive matching and
  struct-literal construction.
- Reject unsupported tokenizer configurations explicitly. CLIP still omits full `ftfy` repair;
  the bundled T5 model does not synthesize Hugging Face extra IDs.

### Migration

Consumers upgrading from 0.4 should apply the following changes. The items are independent unless
their Cargo feature is enabled; consumers using the default features should review the full list.

- Consumers using `default-features = false` must opt into the capability they call. Use `client`
  for a custom transport, `reqwest-transport` for ordinary default HTTP, `streaming` for a custom
  SSE transport, or `reqwest-streaming` for the complete default transport. Enable
  `png-metadata` alone for metadata parsing and `generation` for request planning/sending types.
- Replace `RunDirectorToolRequest { tool, prompt, defry, .. }` with
  `RunDirectorToolRequest { command: DirectorCommand::..., .. }`. `Colorize` carries an optional
  prompt and required defry value; `Emotion` carries a required prompt and defry value. Other
  commands cannot carry those invalid fields.
- Construct image-bearing request fields with `EncodedImage::from_bytes` or
  `EncodedImage::from_base64`. `Img2ImgRequest::new` now takes `EncodedImage`, as do masks,
  character references, Vibe encoding requests and director requests.
- Treat `Client::encode_vibe` as returning `VibeEncoding`; pass it directly to
  `VibeReference::vibe_data_cache`, or use `to_base64()` only at a text persistence boundary.
- Implement `ImagePreprocessor` and call `NormalizedGeneration::prepare_with` only when replacing
  the default pixel transform. Response decode failures now use `DecodeTarget::ImageResponse`.
- Replace generic `Transport::{get_json,post_bytes,post_sse}` implementations with fixed
  `Transport::execute`. Implement `StreamingTransport::execute_sse` only for SSE support. Inspect
  `TransportRequest.body` as serialized JSON and return status/headers/body without API mapping.
- Match the new `RequestOutcome::Cancelled` variant. Exhaust streams to observe success, handle
  yielded errors for failure, or call `GenerateImageStream::cancel().await`; connection setup alone
  is no longer reported as success.
- Select `Model::NaiDiffusion45Full` explicitly to retain the previous default model, and use
  its builder or `SamplingParams::for_model` to retain its sampling defaults.
- Replace `model.capabilities()` / `request.capabilities()` with `descriptor()` and the relevant
  profile getter. Replace `noise_schedule.resolve_for(...)` with
  `descriptor.sampling().resolve_noise_schedule(noise_schedule)`; handle optional inpainting IDs.
  See [model descriptor migration](docs/model-descriptors.md) for field-by-field replacements.
- Persist the serializable `Model` enum, not writable capability snapshots. Descriptors cannot
  be constructed or deserialized externally; detached profile copies do not modify the registry.
- Replace `GenerationCore` / `GenerateImageRequest` `processed_prompt*` calls with
  `request.resolve_prompts()?.prompt` / `.negative_prompt`. The returned text includes presets.
- Handle normalization errors from request/client cost estimates and request token counting;
  `AnlasEstimateInput::for_request` also returns `Result`. For repeated queries, use
  `request.normalize()?` and its cached prompts, token counting and direct pricing projection.
- For repeated estimates, retain `NormalizedGeneration` and use its borrowed cost projection.
  Avoid cloning a request that owns large `EncodedImage` payloads only to expose cost fields. An
  application with its own domain model should keep any extension trait or newtype in its adapter
  instead of making the domain layer depend on bridge client or wire types.
- To inspect diagnostics before sending, use `plan.prepare()?` followed by
  `client.generate_prepared(...)` or `generate_stream_prepared(..., mode)`. Editing requires
  `plan.into_request()` and another normalization pass; plans cannot be deserialized or mutated.
- Use original character-list indices from `ResolvedCharacterPrompt::index` and
  `CharacterTokenUsage::index`; disabled characters are absent from these lists.
- Add fallback arms when matching non-exhaustive enums and `..` when matching their structs.
  External transports construct `ApiError`, `TransportError` and `DecodeError` with `new`, then
  set optional public fields as needed.
- Handle `BridgeError::Tokenizer` when `tokenizer` is enabled, and
  `InvalidRequest::PromptTokenLimitExceeded` for strict token-limit checks.
- Treat arbitrary or partially corrupt raw/ZIP generation and Director responses as
  `DecodeTarget::ImageResponse` failures. Best-effort `GeneratedImageMetadata` states apply only
  after the image itself has passed PNG, JPEG or WebP validation. Continue to read the request seed
  from `GenerateImageResult::resolved_seed` and optional per-image seeds from
  `GeneratedImage::seed()`.

## 0.4.0 - 2026-08-27

### Fixed

- `SubscriptionInfo::v5_usage` no longer requires `usage.timeUntilNextPercent`. Only
  `usage.percent` decides whether an allowance is reported; a missing recharge rate degrades to
  `0`. Previously the whole object was discarded, which also discarded `isNegative` and therefore
  silently restored the Opus first-image allowance for an overdrawn V5 account.

### Documented

- Verified the `usage` object against a live Opus account (2026-08-27), resolving the open question
  from 0.4.0-rc.1. `docs/anlas-pricing.md` gains a "V5 Opus generation allowance" section: `percent`
  is the remaining allowance as a whole integer, `timeUntilNextPercent` is a recharge *rate*
  constant rather than a countdown (7888 s per percent, stable across a `percent` change), and the
  V5 free first image draws on this allowance instead of Anlas. Twelve consecutive
  `nai-diffusion-5-full` generations at 832×1216 / 23 steps cost 0 Anlas and moved `percent` from
  100 to 99, so one such generation costs ~0.11%–0.12% of the meter.

## 0.4.0-rc.3

### Added

- Add `ModelCapabilities::supports_streaming`. NovelAI Diffusion V4, V4.5, and V5 support image
  streaming; V3 and Furry V3 do not.

### Changed

- `Client::generate_stream` rejects V3 and Furry V3 locally with
  `InvalidRequest::UnsupportedModelFeature` before any HTTP request. This capability mismatch
  never falls back to a non-streaming request, regardless of `strict_mode`.

### Migration

- Add `supports_streaming` when constructing `ModelCapabilities` directly. Callers that offer a
  stream toggle should gate it through `Model::capabilities().supports_streaming`; direct stream
  calls must handle `UnsupportedModelFeature` for V3 models.

## 0.4.0-rc.2

Breaking release candidate refining Anlas pricing and credit observation.

### Added

- Update Anlas pricing for the NAI 3, 4, 4.5, 5 web formula: raw-pixel pricing, SMEA/V5 multipliers,
  resolution sample caps, Opus first-image eligibility, character-reference and vibe surcharges,
  and an explicit over-140 estimate status. `GenerateImageRequest` and `Client` now expose the
  same calculator mapping, while older and unrelated model pricing remains out of scope.
- Add `AnlasPricingContext` and `RequestCostEstimate` for account-sensitive pricing and uniform
  observed-request totals.

### Changed

- `CreditEvent::RequestPlanned::estimate` now uses `RequestCostEstimate`, separating generation
  breakdowns from fixed encode-vibe and director-tool charges.
- `estimate_director_tool_cost` now accepts `AnlasPricingContext`, so an expired Opus subscription
  no longer receives the free director allowance.
- `AnlasEstimate::free` is replaced by the derived `is_free()` method, and the sample breakdown now
  distinguishes requested, priced and billable counts.
- Generation cost multiplier, Opus usage limiting and character-reference inpainting support now
  come from `ModelCapabilities` instead of duplicated model checks.

### Migration

- Replace `estimate_director_tool_cost(size, is_opus)` with
  `estimate_director_tool_cost(size, context)`. Build an `AnlasPricingContext` from the latest
  subscription or balance state, or use `Default::default()` when that state is unavailable.
- Handle `CreditEvent::RequestPlanned::estimate` as either
  `RequestCostEstimate::Generation(estimate)` or `RequestCostEstimate::Fixed { anlas }`.
- Replace `estimate.free` with `estimate.is_free()`. Update `AnlasEstimateInput` initializers to
  use `context` instead of `is_opus`, and update `AnlasEstimate` literals because it no longer
  implements `Default`.
- Add the new fields when constructing `RequestFingerprint`, `CreditBalance`, `SubscriptionInfo`
  or `ModelCapabilities` directly.

## 0.4.0-rc.1

Breaking release candidate. It adds NovelAI Diffusion V5, replaces model-version branching with a
capability registry, fixes image-to-image and inpainting request assembly, and adds optional
credit observation.

### Added

- Support `nai-diffusion-5-full` and `nai-diffusion-5-curated`, including `params_version: 4`,
  `tag_hint_transparent_background` and `legacy_uc` in the request payload. V5 supports neither
  vibe transfer nor character reference, and drops variety boost.
- Add `ModelCapabilities` and `Model::capabilities()`. Every model-specific decision in request
  assembly and validation now reads this one registry, so adding a model family is one entry.
- Add inpainting support: a request carrying `Img2ImgRequest::mask` is sent as `action: "infill"`
  against the model's inpainting sibling, exposed as `Model::inpainting_id()`.
- Add `CreditObserver`, `CreditEvent`, `RequestFingerprint`, `CreditBalance` and
  `estimate_anlas_cost` / `estimate_director_tool_cost`. Attaching an observer through
  `ClientOptions::credit_observer` or `Client::with_credit_observer` reports a de-identified
  fingerprint plus a local cost estimate before each request and its outcome afterwards. The
  crate persists nothing itself.
- Add `SubscriptionInfo::v5_usage` for the free V5 generation allowance reported by
  `/user/subscription`, and `Client::last_sampled_balance()`.
- Add `GenerateImageRequest::builder()`.
- Add `GenerationCore::transparent_background`, a V5-only toggle that appends a
  `transparent background` tag and sets `tag_hint_transparent_background` to match. Measured at
  ~39% fully transparent pixels on a live V5 generation.
- Add `QualityPreset` with `Standard` / `Light` / `None`, replacing the `quality: bool` toggle.
  `Standard` is byte-identical to the old `quality: true` tags and `None` to `quality: false`.
  `Light` swaps `masterpiece` for `amazing quality` and is V5 only, gated by
  `ModelCapabilities::supports_light_quality_preset`.
- Add samplers `k_dpmpp_2m_sde` and `ddim_v3`, and the `native` noise schedule. `native` is
  rewritten to `karras` for V4 and later, which reject it.
- Add `CharacterReferenceType::Costume` and `CharacterReferenceType::Delta`.

### Fixed

- **Image-to-image and inpainting requests were rejected by NovelAI.** `action` was hardcoded to
  `"generate"`, so any request carrying `image` / `strength` / `noise` failed parameter
  validation server-side. The action is now derived from the request shape
  (`generate` / `img2img` / `infill`).
- Fit image-to-image sources and inpaint masks to the requested canvas before sending. NovelAI
  requires the source to match `width` / `height`; previously the payload was only base64-checked.
- Send `extra_noise_seed` and `color_correct` for image-to-image requests.
- Send the caller's strength through `inpaintImg2ImgStrength` for infill requests instead of a
  hardcoded `1.0`.
- Compute `skip_cfg_above_sigma` as `coefficient * sqrt(pixels / (832 * 1216))` with a
  model-dependent coefficient (19 for V4, 58 for V4.5) instead of a hardcoded integer `58`. V5
  does not support variety boost and never receives the field.
- Correct the Furry V3 model identifier from `nai-diffusion-3-furry` to `nai-diffusion-furry-3`.
- Merge the UC preset text *before* the caller's negative prompt and include the leading `nsfw`
  tag, matching the NovelAI web client. Previously the preset was appended after.
- Omit parameters a model family does not accept instead of sending them as `null`. The
  `skip_serializing_none` attribute was placed after `derive(Serialize)`, where it had no effect.
- Only send `normalize_reference_strength_multiple` when vibe references are present.
- Only send `sm`, `sm_dyn` and `dynamic_thresholding` for V3-family models.
- Drop vibe transfer from infill requests, which NovelAI answers with a server error.

### Changed

- `GenerateImageRequest` is now grouped by capability: `core`, `sampling`, `characters`,
  `guidance`, `img2img`, `references`, `output`, `strict_mode`.
- `ControlNetConfig` / `ControlNetInput` are renamed to `VibeTransferConfig` / `VibeReference`.
  The wire field names are unchanged.
- Unsupported capability fields now follow `strict_mode`: rejected with
  `InvalidRequest::UnsupportedModelFeature` when strict, dropped with a `log::warn!` otherwise.
- Sampling defaults follow the selected model (V4.5 and V5 use scale 5.0, V4 uses 5.5, Furry V3
  uses 6.2).
- Canvases above the 3145728 pixel budget are shrunk on the 64 pixel grid, or rejected in
  `strict_mode`.
- `Model::vibe_model_key` returns `Option<&'static str>` and yields `None` for V5, which has no
  vibe support.
- `Client::encode_vibe` rejects a model whose capabilities report no vibe support, instead of
  sending a request that cannot succeed.

### Migration

- Replace flat field access with the grouped equivalents, or use the builder:

  ```rust
  // 0.3
  let request = GenerateImageRequest {
      prompt: "1girl".to_owned(),
      model: Model::NaiDiffusion45Full,
      seed: 42,
      variety_boost: true,
      ..Default::default()
  };

  // 0.4
  let request = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
      .seed(42)
      .variety_boost(true)
      .build();
  ```

- Field moves: `prompt`, `negative_prompt`, `model`, `size`, `seed`, `n_samples`, `quality` and
  `uc_preset` are on `core`; `steps`, `scale`, `sampler`, `noise_schedule` and `cfg_rescale` are
  on `sampling`; `variety_boost` is on `guidance`; `image_format` is on `output`.
- `i2i` is renamed to `img2img` and gained `color_correct`, `extra_noise_seed` and
  `add_original_image`. Use `Img2ImgRequest::new` to pick the client defaults.
- `characters` is now `Option<CharacterSet>`, which also carries the former top-level
  `use_coords`.
- `character_references` and `controlnet` moved into `references: Option<ReferenceSet>` as
  `character_references` and `vibe_transfer`.
- `VibeReference::info_extracted` is removed. Pass the value to `encode_vibe`, which bakes it
  into the cache; generation requests carry no separate field for it.
- `GenerationCore::quality` and the builder's `.quality()` take a `QualityPreset` instead of a
  `bool`. Replace `true` with `QualityPreset::Standard` and `false` with `QualityPreset::None`.
- Callers pinning `Model::NaiDiffusion3Furry` by its serialized name must update stored values
  from `nai-diffusion-3-furry` to `nai-diffusion-furry-3`.
- Negative prompts change: the UC preset now precedes the caller's text and starts with `nsfw`.
  Select `UcPreset::None` to send the caller's negative prompt verbatim.

### Verified against the live API

Settled with `cargo xtask probe` against a real key, at a fixed seed (generation is
pixel-deterministic, so A/B runs are comparable):

- `reference_information_extracted_multiple` is **not** required and has **no effect**. Omitting
  it, sending `[0.1]` and sending `[1.0]` all produce pixel-identical output, because
  `encode_vibe` bakes the value into the returned cache. The field is no longer sent, and
  `VibeReference::info_extracted` is removed rather than kept as an input that does nothing.
- `add_original_image: true` is correct for infill. With `true` the region outside the mask is
  pixel-identical to the source; with `false` it drifts by a mean of ~1.8/255 from a full-canvas
  VAE round trip. The masked region is unaffected either way.
- `tag_hint_transparent_background` is inert: `true` and `false` produce pixel-identical output.
  Transparency comes from the `transparent background` prompt tag alone, which takes ~46% of the
  canvas to alpha 0. V5 returns RGBA either way. The field is still sent, now tracking the
  request's actual value rather than a hardcoded `true`.
- Request shapes accepted with HTTP 200: V5 text-to-image, V5 image-to-image, V4.5
  image-to-image, V4.5 infill, and V4.5 `encode_vibe` plus vibe-transfer generation.

Still unverified: the `usage` object carrying the free V5 allowance never appeared on the
account used for probing, so `SubscriptionInfo::v5_usage` is parsed from the documented field
names but has not been seen populated. (Resolved 2026-08-27 — see 0.4.0.)

## 0.3.1

- Validate complete PNG image data through `IEND` before reporting parsed metadata.
- Read PNG text chunks that appear after image data.
- Preserve bounded `zTXt` / `iTXt` decoding failures as structured metadata warnings.

## 0.3.0

- Change `Client::generate` to return `GenerateImageResult`, separating the request-level
  `resolved_seed` from individual images.
- Parse every returned PNG independently and derive per-image seeds only from embedded metadata.
- Add `GeneratedImageMetadata` and `inspect_generated_image_metadata` for non-failing, diagnosable
  output metadata inspection.
- Change `Client::generate_stream` to return `GenerateImageStream`, which exposes the request-level
  `resolved_seed` and continues to implement `Stream`.
- Add serialization support and generation-data inspection helpers for parsed PNG metadata.
- Preserve recoverable metadata diagnostics such as malformed `Comment` JSON in
  `PngMetadata::warnings`.

### Migration

- Replace direct iteration over `client.generate(...).await?` with iteration over
  `client.generate(...).await?.images`.
- Read the actual request seed from `GenerateImageResult::resolved_seed`.
- Replace `GeneratedImage::seed` field access with `GeneratedImage::seed()`. It now returns only an
  embedded per-image seed and never falls back to the request seed.
- Existing `StreamExt` consumers can continue polling the value returned by `generate_stream`;
  read its `resolved_seed` field before or while consuming chunks.
