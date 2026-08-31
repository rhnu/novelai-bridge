# Generation wire baseline

`generation.json` captures valid default requests at `8fd7675` (`0.5.0-rc.1`).
The offline mock transport records eight ordinary requests and six supported streaming requests.
All use prompt `1girl` and fixed seed `42`. JSON object ordering is insignificant; every field,
omission, value and array order is checked. Do not automatically regenerate this file when a test
fails. Review intended protocol changes separately. Existing generation tests cover img2img,
inpainting, character positioning and reference payloads with real generated image fixtures.

## Known regressions

`tests/architecture_regressions.rs` asserts desired contracts, not current bugs. Initially all six
regressions were confirmed to fail by running that target with `--ignored`. Each has a reason naming
its owning area. Remove the ignore when its implementation is fixed; do not invert the assertion.
Model descriptor, stream lifecycle and response validation are separate follow-up changes.

Successful mock responses use valid PNG/WebP bytes. A malformed PNG Comment JSON test uses a valid
image with a recoverable metadata warning, not a truncated image disguised as a success fixture.

## Dependency and conversion baseline

`cargo tree --no-default-features --locked --offline --depth 1` at the baseline includes 17 normal
dependencies: async-stream, async-trait, base64, eventsource-stream, futures-core, futures-util,
httpdate, image, log, png, reqwest, secrecy, serde, serde_json, serde_with, thiserror and zip.
Only tokenizer-specific direct dependencies disappear. Image and ZIP retain their default codec
features. This is an observed dependency baseline, not a compile-time or memory benchmark.

Before normalization refactoring, generating with tokenizers and a credit observer resolves
prompts three times (counting, wire, observation). Public estimation maps the raw request separately
from the client's normalization. Base64 normalization decodes once to validate and again to obtain
bytes; character-reference preparation fully decodes once for dimensions and again for fitting.
Wire assembly clones prepared reference arrays. Subsequent work should verify these paths rather
than claiming zero-copy or performance gains from file moves alone.
