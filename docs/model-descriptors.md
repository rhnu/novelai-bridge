# Model descriptors

`Model::descriptor()` returns the immutable registry entry for a supported base model. Descriptors contain model knowledge, not transport state, image payloads or tokenizer assets. Model serialization and API identifiers use the same registry mapping.

## Responsibilities

| Profile | Responsibility |
| --- | --- |
| `identity()` | Base model and API identifier |
| `support()` | Streaming and optional inpainting/reference profiles |
| `constraints()` | Canvas and numeric bounds |
| `defaults()` | Core and sampling defaults used by request builders |
| `prompt()` | Encoder kind, per-field limit and conditioning support |
| `sampling()` | Noise schedules, fallback and sampling capabilities |
| `wire()` | Conditioning representation and protocol extensions |
| `pricing()` | Model multiplier and Opus allowance policy |

Registry entries compose small profiles. Inpainting support carries its sibling identifier, character-reference support carries its allowed modes, and variety boost carries its coefficient. A missing profile means the capability is unavailable.

Request factories read `model.descriptor().defaults()`; basic parameter values do not consult the registry. When composing sampling manually, use `model.descriptor().defaults().sampling`.

## Request availability

`request.mode()` derives TextToImage, ImageToImage or Inpainting from its input. `descriptor.model_id_for(mode)` returns the corresponding supported identifier.

`descriptor.resolve_references(mode, ReferenceIntent { vibe, character })` is a pure query. Its results distinguish available, not requested, unsupported, unimplemented input, unsupported mode and conflict with character references.

Character support is resolved first. Unsupported character input cannot suppress valid vibe input. Empty reference subgroups are treated consistently before availability and retained-value validation. Reject policy turns unavailable input into an error; Normalize removes it and records diagnostics.

V3 retains provider-only legacy vibe metadata knowledge, but its generation input and encode-vibe operation are not implemented. Use `can_encode_vibe()` for encoding capability. V4/V4.5 support encoded input; V5 has neither vibe nor character-reference profiles.

## Effective plans

Normalization assembles effective positive/negative text and enabled character conditioning once. Each resolved character retains its original index and effective coordinate. V4/V4.5 use the five-by-five grid; V5 keeps normalized freeform coordinates. Wire adaptation uses these values without projecting coordinates again.

V5 allows 32 enabled character prompts; V4/V4.5 allow six. Normalize disables enabled overflow while preserving original indices; Reject reports it. Disabled characters do not participate in validation, token counting or wire conditioning.

Quality presets, transparency, furry mode and sampling support follow the selected descriptor. Native noise scheduling on V4+ retains its Karras fallback and reports the change.

`NormalizationDiagnostic::field` is a typed `GenerationField`. Its display value is the complete editable request path, including original character indices. Conflict diagnostics identify the retained field with the same type.

Persist models and editable requests. Registry descriptors and prepared values are not writable configuration; edit a request and normalize it again to change effective behavior.
