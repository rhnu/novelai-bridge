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

For a read-only formatting check, use `cargo dev format --check --diff --locked`. Feature checks compile each independent capability and its important intersections. They do not contact NovelAI or run live probes. Top-level integration scenarios are compiled by the first command but are separate from the pure unit test command.

Markdown uses [rumdl](https://rumdl.dev/) and the repository's `.rumdl.toml`. Line-length enforcement and automatic prose reflow are disabled. After editing documents, pass the changed files to `rumdl check --fix`.

## Request inspection

```sh
cargo xtask capabilities
cargo xtask shape t2i --model nai-diffusion-5-full
cargo xtask shape stream --seed 123
```

`shape` consumes the same prepared payload as the client and does not construct a transport or require credentials. Vibe shape inspection covers the encode-vibe request; generation with that encoding needs its response.

`cargo xtask probe` sends the selected scenario and can consume credits. It resolves `NOVELAI_API_KEY` from the environment or the repository-local `.env`. The `--set` and `--unset` options apply deliberate JSON edits after preparation. Setting can add an object field; missing parent paths and removal targets are errors.

`scripts/probe_user_api.py` remains an exploratory tool for raw user-service endpoints, including endpoints outside the typed client's API. It prints redacted response shapes and is not part of library execution or automated verification.

## Tokenizer references

`cargo xtask fetch-tokenizer-assets` downloads the revisions in the asset package's `SOURCES.toml`, checks hashes and writes deterministic gzip files. Library builds and encoding never fetch assets.

`scripts/tokenizer-golden.py` regenerates exact token-ID references using independent upstream Python implementations. Its dependencies are listed in the script. Keep asset provenance, licenses and reference fixtures aligned when changing a pinned revision.

The asset package owns embedded bytes and integrity checks. Runtime parsing and encoder caches belong to the main library.
