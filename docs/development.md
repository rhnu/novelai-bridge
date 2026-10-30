# Development

## Tools

Install Rust with rustfmt and Clippy. Formatting and lint use [Rusteward](https://github.com/rhnu/rusteward):

```sh
cargo install --git https://github.com/rhnu/rusteward.git --locked rusteward
```

`rusteward.toml` is the workspace profile. It keeps the project's nursery/cargo Clippy checks alongside Rusteward's managed rules. Authored modules use `module.rs` and a matching child directory; pure unit tests live in child `tests.rs` modules.

## Verification

Run compilation, formatting, lint and pure unit tests in that order:

```sh
cargo check --workspace --all-targets --all-features --locked
cargo dev format --locked
cargo dev lint --locked
cargo test --workspace --lib --bins --all-features --locked
cargo xtask check-features
```

For a read-only formatting check, use `cargo dev format --check --diff --locked`. Feature checks compile all six effective combinations of the HTTP adapter, tokenizer and bundled assets. They do not contact NovelAI or run live probes. Top-level integration scenarios are compiled by the first command but are separate from the pure unit test command.

Markdown uses [rumdl](https://rumdl.dev/) and the repository's `.rumdl.toml`. Line-length enforcement and automatic prose reflow are disabled. After editing documents, pass the changed files to `rumdl check --fix`.

## Request inspection

```sh
cargo xtask capabilities
cargo xtask shape t2i --model nai-diffusion-5-full
cargo xtask shape stream --seed 123
cargo xtask shape upscale --size 256x256
cargo xtask shape tags --prompt cat --lang en
```

`shape` consumes the same prepared payload and `ApiEndpoint` request description as the client and does not construct a transport or require credentials. Vibe shape inspection covers the raw V3 generation request or the V4/V4.5 encode-vibe request; generation with an encoding needs its response. Use `shape encode-vibe` or `probe encode-vibe` to inspect or send only the encoding operation. Encoding accepts `--information-extracted`, `--mask`, `--crop-to-mask`, `--focus-seed` and `--info-extract-seed`. Transparent generation accepts `--alpha straight` or `--alpha premultiplied`.

`cargo xtask probe` sends the selected scenario and can consume credits. It resolves `NOVELAI_API_KEY` from the environment or the repository-local `.env`. The `--set` and `--unset` options apply deliberate JSON edits after preparation. Setting can add an object field; missing parent paths, removal targets and body edits on a bodyless operation are errors. Synthetic image dimensions and pixel storage are checked before allocation.

Select `--stream-mode msgpack`, `--response-format json`, `--request-encoding multipart` and
`--cache-images` to exercise the corresponding paths. `--repeat N` reuses one client, so later
runs can use its registered keys; every run can consume credits. `shape` prints the semantic JSON
before multipart/cache encoding. JSON overrides require JSON request encoding. Multipart probe
output omits binary content and keys, and JSON inspection redacts cache-secret fields.

The tool installs a tracing subscriber on stderr with an `info` default; `RUST_LOG` selects another filter and invalid filters fail at startup. Inspection output deliberately includes prompt text. Client tracing uses content-free summaries and does not log server error bodies, which may echo user data.

`scripts/probe_user_api.py` remains an exploratory tool for raw user-service endpoints, including endpoints outside the typed client's API. It prints redacted response shapes and is not part of library execution or automated verification.

## Tokenizer references

`cargo xtask fetch-tokenizer-assets` downloads the revisions in the asset package's `SOURCES.toml`, checks hashes and writes deterministic gzip files. Library builds and encoding never fetch assets.

`scripts/tokenizer-golden.py` regenerates exact token-ID references using independent upstream Python implementations. Its dependencies are listed in the script. Keep asset provenance, licenses and reference fixtures aligned when changing a pinned revision.

The asset package owns embedded bytes and integrity checks. Runtime parsing and encoder caches belong to the main library.

## Prompt syntax

`crates/prompt` owns pure syntax scanning and semantic emphasis analysis; the main SDK's
model registry owns syntax capability selection. It intentionally has no dependency on
the main SDK, media processing, transports or tokenizers. Compile it independently with
`cargo check -p novelai-bridge-prompt --target wasm32-unknown-unknown --all-features --locked`
when changing its portable contract. Its pure unit tests live below `src/analysis/tests.rs`.
The optional `typegen` feature only derives `ts_rs::TS`; it does not generate files during builds.

## Backend boundaries

Editable requests, normalized plans and prepared payloads represent different guarantees. Normalize once to decide model identity, applicable mode fields, conditioning and pricing inputs; wire serialization projects that immutable state. Operation-specific input variants prevent image-to-image options from silently disappearing when a mask is added.

Media budgets are shared below metadata and pixel processing. PNG validation must reach the end of the file; a pixel decoder that stops after the first frame cannot establish complete PNG validity. Structural failures reject media, while supported text that cannot be interpreted produces a diagnostic. Prepared media retains measurements so a custom constructor cannot bypass a stricter request budget or force another pixel decode.

Transports return HTTP status, headers and raw byte streams. The protocol layer owns bounded body reads, SSE/MessagePack framing and image event decoding; client response budgets apply to every transport. Successful responses retain their headers through operation decoding.

A stream handle owns both consumption and terminal callback delivery. Its settlement future must survive cancellation of a single `next()` future; explicit cancellation completes an existing settlement instead of replacing its outcome. Do not move that future into a temporary polling wrapper. EOF establishes success only after all expected samples have validated final images. Unknown events retain their complete protocol-specific source and cannot establish completion; malformed official events fail rather than falling back to Unknown.

Domain and tokenizer layers retain error sources and typed field locations. Client operation boundaries own failure logging and span context; the executable owns subscriber configuration. API error bodies remain available on returned errors but stay out of logs because a server can echo submitted content.
