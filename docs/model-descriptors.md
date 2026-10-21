# Model descriptors

`Model::descriptor()` returns the immutable registry entry for a supported base model. Descriptors contain model knowledge, not transport state, image payloads or tokenizer assets. Model serialization and API identifiers use the same registry mapping.

## Responsibilities

| Profile | Responsibility |
| --- | --- |
| `identity()` | Base model and API identifier |
| `support()` | Streaming and optional inpainting/reference profiles |
| `constraints()` | Canvas and numeric bounds |
| `defaults()` | Core and sampling defaults used by request builders |
| `prompt()` | Encoder kind, per-field limit, model-specific presets and conditioning support |
| `sampling()` | Noise schedules, fallback and sampling capabilities |
| `wire()` | Conditioning representation and protocol extensions |
| `pricing()` | Model multiplier and Opus allowance policy |

Registry entries compose small profiles. Inpainting support carries its sibling identifier, character-reference support carries its allowed modes, and variety boost carries its coefficient. A missing profile means the capability is unavailable.

Request factories read `model.descriptor().defaults()`; basic parameter values do not consult the registry. When composing sampling manually, use `model.descriptor().defaults().sampling`.

## Request availability

`GenerateImageRequest::operation` selects one `GenerationInput` variant:

- `TextToImage` has no source image.
- `ImageToImage(ImageToImageInput)` carries a source image and its color-correction and noise-seed options.
- `Inpainting(InpaintingInput)` carries a source image, required mask and source-preservation option. Its optional `img2img` group adds strength, noise, color correction and a noise seed on models whose inpainting profile supports it.

Use the builder's `operation`, `image_to_image` or `inpainting` method to select the operation. `InpaintingInput::new(image, mask)` selects pure infill. Set `img2img` to `Some(InpaintImageToImage::new(strength, noise))` for source conditioning. The nested group owns color correction and the optional noise seed; omitted seeds reuse the resolved request seed. V3 supports pure infill; V4 and later also support the nested group.

`request.mode()` delegates to the selected input. `descriptor.model_id_for(mode)` queries the supported API identity; normalization resolves it once, exposed as `NormalizedGeneration::model_id()`. Unsupported operations are rejected rather than converted to another operation.

Editable generation requests and operation envelopes reject unknown serialized fields. The operation uses a `mode` tag and an `input` object for image-to-image or inpainting. Unknown keys are rejected rather than ignored. See the [release notes](../CHANGELOG.md) for migration details.

`descriptor.resolve_references(mode, ReferenceIntent { vibe, character })` is a pure query. Its results distinguish available, not requested, unsupported, unimplemented input, unsupported mode and conflict with character references.

Character support is resolved first. Unsupported character input cannot suppress valid vibe input. Empty reference subgroups are treated consistently before availability and retained-value validation. `request.policy` uses the shared `ValidationPolicy`: `Reject` turns unavailable reference input into an error; `Normalize` removes it and records diagnostics. `normalize_with(policy)` overrides the editable request policy for the resulting plan.

V3 exposes legacy vibe metadata, but this bridge cannot use those vibe references for generation or encode new V3 vibe data. Use `can_encode_vibe()` for encoding capability. V4/V4.5 support encoded input; V5 has neither vibe nor character-reference profiles.

## Effective plans

Normalization assembles effective positive/negative text and enabled character conditioning once. Each resolved character retains its original index and effective coordinate. V4/V4.5 use the five-by-five grid; V5 keeps normalized freeform coordinates. Wire adaptation uses these values without projecting coordinates again.

The selected prompt profile supplies the enabled-character limit. Normalize disables enabled overflow while preserving original indices; Reject reports it. Disabled characters do not participate in validation, token counting or wire conditioning.

Quality and UC preset definitions belong to each model's prompt profile. They determine supported selections, effective text and the wire hints. Use `quality_preset` and `uc_preset` to inspect a supported selection; preset enums alone do not supply universal text. Full and curated models can use different presets and defaults. V3's default UC selection is Heavy.

Transparency, furry mode and sampling support follow the selected descriptor. V3 supports Variety+ with its own sigma profile. When a model does not support native noise scheduling, Normalize selects its Karras fallback and records the change; Reject returns an unsupported-feature error.

`NormalizationDiagnostic::field` is a typed `GenerationField`. Local request errors and image preprocessing use the common `RequestField`, with generation locations carried by `RequestField::Generation`. Token-limit errors use these same locations. Display values identify the editable request path, including original character indices and image fields such as `operation.input.mask`. Conflict diagnostics identify the retained field with a `GenerationField`.

Preparation resolves automatic seeds and transforms image pixels. Failures to obtain a random seed, encode a prepared image or satisfy its required canvas use `BridgeError::Preparation`. Debug summaries omit prompt text and image payloads.

Persist models and editable requests. Registry descriptors and prepared values are not writable configuration; edit a request and normalize it again to change effective behavior.
