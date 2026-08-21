# Changelog

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
- Request shapes accepted with HTTP 200: V5 text-to-image, V5 image-to-image, V4.5
  image-to-image, V4.5 infill, and V4.5 `encode_vibe` plus vibe-transfer generation.

Still unverified: the `usage` object carrying the free V5 allowance never appeared on the
account used for probing, so `SubscriptionInfo::v5_usage` is parsed from the documented field
names but has not been seen populated.

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
