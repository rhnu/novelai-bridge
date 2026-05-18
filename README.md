# novelai-bridge

`novelai-bridge` is a Rust library for NovelAI image-generation APIs and a small set of related helpers.

Scope:

- `generate`
- `generate_stream`
- `encode_vibe`
- `augment-image` director tools
- `get_subscription`
- PNG metadata parsing
- embedded vibe document extraction
- local image payload normalization / reference preprocessing helpers

Example:

```rust
use novelai_bridge::{ApiKeySource, BaseUrls, Client, ClientOptions, GenerateImageRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(ClientOptions {
        api_key_source: ApiKeySource::Environment,
        base_urls: BaseUrls::default(),
        ..Default::default()
    })?;

    let images = client
        .generate(GenerateImageRequest {
            prompt: "1girl".to_owned(),
            ..Default::default()
        })
        .await?;

    println!("generated {} image(s)", images.len());
    Ok(())
}
```

Disclaimer:

- This project is under experimental development and does not guarantee API or behavior stability.
- Development of this project has been assisted by Codex.

Notes:

- This crate does local shape/range validation, not prompt syntax parsing.
- The default transport uses `reqwest + rustls`.
- This project is not affiliated with NovelAI.

Credits: see [CREDITS.md](CREDITS.md).

License: [GPL-3.0-only](LICENSE)
