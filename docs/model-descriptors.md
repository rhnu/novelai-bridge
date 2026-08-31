# Model descriptors

`Model::descriptor()` returns `&'static ModelDescriptor`. The descriptor owns no transport,
payload or tokenizer assets. It cannot be constructed, mutated or deserialized by callers.
`GenerateImageRequest::descriptor()` delegates to its selected model.

## Responsibilities

| Profile | Responsibility |
| --- | --- |
| `identity()` | Base model and API identifier |
| `support()` | Streaming and optional inpainting/reference profiles |
| `constraints()` | Canvas and numeric bounds consumed by normalization |
| `defaults()` | Factory core and sampling values consumed by builders |
| `prompt()` | Encoder, independent context limit, character count and prompt options |
| `sampling()` | Noise schedules, fallback, SMEA/decrisp and optional variety boost |
| `wire()` | Conditioning representation, parameter version and protocol extensions |
| `pricing()` | Model multiplier and Opus allowance policy |

Registry entries explicitly combine small profiles. There is no cross-generation whole-descriptor
inheritance. Inpainting exists only with a sibling ID; character reference support exists only with
a mode profile; variety boost exists only with its coefficient. Missing profiles mean unsupported
capabilities. Shared profiles do not imply inheritance of unrelated features.

These are not bitfields: current consumers need parameters and request-context decisions, not bit
set operations. No per-model capability traits or bare public bit layouts are introduced.

## Static support versus request availability

`GenerationMode` is `TextToImage`, `ImageToImage` or `Inpainting`.
`request.mode()` derives the raw shape; `descriptor.model_id_for(mode)` selects a supported API
identity and returns `None` if the mode has no registered sibling.

`descriptor.resolve_references(mode, ReferenceIntent { vibe, character })` is a pure query.
It returns a decision for each requested group:

- `Available` / `NotRequested`;
- `Unsupported` when the provider capability is absent;
- `InputNotImplemented` for a provider-only input path;
- `UnsupportedMode` for a mode rejected by the model or reference profile;
- `ConflictWithCharacterReference` when a valid character reference takes precedence over vibe.

Character support is resolved first. An unsupported character input cannot suppress otherwise
valid vibe input. Normalization uses these same decisions: Reject converts them to local errors;
Normalize drops rejected groups and reports diagnostics. The query does not inspect payloads,
validate numeric ranges, count tokens or make requests.

V3's `VibeInputPath::ProviderOnlyLegacy` preserves provider/metadata information, but the bridge
does not implement its generation input or encode-vibe operation. Use `can_encode_vibe()` for that
operation, not `vibe_model_key().is_some()`. V4/V4.5 use encoded input; V5 has neither vibe nor
character-reference profiles. Character-reference modes are also intersected with model mode
support, so an optional reference profile cannot create an inpainting sibling.

## Migration

The default model is now `Model::NaiDiffusion5Full`, including default generation and estimate
inputs. Default sampling follows V5 Full (guidance scale 7). To keep the old behavior, choose
`Model::NaiDiffusion45Full` explicitly in the builder; when composing fields manually, use
`SamplingParams::for_model(model)` alongside `GenerationCore::new(prompt, model)`.

The old `ModelCapabilities`, `Model::capabilities()` and request `capabilities()` are removed.
Given `let d = model.descriptor();`:

| Previous query | Replacement |
| --- | --- |
| `default_steps`, `default_scale` | `d.defaults().sampling.steps`, `.scale` |
| `tokenizer`, `prompt_token_limit` | `d.prompt().tokenizer`, `.token_limit` |
| `max_characters`, `supports_characters()` | `d.prompt().max_characters()`, `.characters.is_some()` |
| `supports_inpainting` | `d.support().inpainting.is_some()` |
| `supports_streaming` | `d.support().streaming` |
| `supports_vibe_transfer` | `d.support().vibe.is_some()` (provider capability only) |
| `supports_encoded_vibe` | `d.can_encode_vibe()` |
| Character-reference flags | `d.support().character_reference`; query `resolve_references` for modes |
| Variety flag and coefficient | `d.sampling().variety` / `variety_sigma(width, height)` |
| SMEA / dynamic thresholding | `d.sampling().smea` / `.dynamic_thresholding` |
| Light quality / transparency | `d.prompt().quality.supports(QualityPreset::Light)` / `.transparent_background` |
| Prompt structure / parameter version | `d.wire().prompt_structure()` / `.params_version()` |
| V5 protocol extensions | `d.wire().uses_v5_extensions()` |
| Pricing multiplier / usage limit | `d.pricing().multiplier` / `.opus_allowance` |

`Model::inpainting_id()` now returns `Option<&'static str>`; it delegates to the same profile that
declares inpainting support. `Model::as_str()` and `vibe_model_key()` also delegate to the registry.
The `Model` enum's serialized identifiers are unchanged.

Replace `schedule.resolve_for(...)` with
`d.sampling().resolve_noise_schedule(schedule)`. Native-to-Karras fallback on V4+ is unchanged
(including Reject policy) but no longer inferred from prompt representation.

Persist `Model` and editable request data. If an application needs a serializable UI snapshot,
project the fields it needs into its own DTO; registry descriptors are not writable configuration.
A detached profile copy is inspectable and editable, but cannot override the active registry.

For explicitly selected models, this change preserves valid wire payloads, independent Anlas
formula results and tokenizer framing.
It does not change streaming lifecycle, response validation or Cargo feature boundaries.
