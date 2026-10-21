//! Explicit, reproducible asset preparation. Never runs during library builds or encoding.
use std::{
    fmt::Write as _,
    io::{Read, Write},
    path::Path,
};

use anyhow::Context as _;
use flate2::{Compression, GzBuilder, read::GzDecoder};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Sources {
    asset: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    sha256: String,
    output: String,
    #[serde(default)]
    upstream_gzip: bool,
}

pub async fn fetch() -> crate::TaskResult<()> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/tokenizer-assets/assets");
    let sources: Sources = toml::from_str(
        &std::fs::read_to_string(directory.join("SOURCES.toml"))
            .context("read tokenizer asset sources")?,
    )?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;
    for asset in sources.asset {
        if Path::new(&asset.output).components().count() != 1 || asset.output.contains(['/', '\\'])
        {
            anyhow::bail!("asset output must be a filename");
        }
        let target = directory.join(&asset.output);
        let upstream = client
            .get(&asset.url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let mut digest = String::with_capacity(64);
        for byte in Sha256::digest(&upstream) {
            write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
        }
        if digest != asset.sha256 {
            anyhow::bail!("SHA-256 mismatch for {}: {digest}", asset.url);
        }
        let bytes = if asset.upstream_gzip {
            // Read the entire stream, including its checksum, before accepting it.
            GzDecoder::new(upstream.as_ref()).read_to_end(&mut Vec::new())?;
            upstream.to_vec()
        } else if Path::new(&asset.output)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
        {
            let mut gzip = GzBuilder::new()
                .mtime(0)
                .write(Vec::new(), Compression::best());
            gzip.write_all(&upstream)?;
            gzip.finish()?
        } else {
            upstream.to_vec()
        };
        if target.exists()
            && std::fs::read(&target)
                .with_context(|| format!("read tokenizer asset {}", target.display()))?
                != bytes
        {
            anyhow::bail!(
                "refusing to overwrite changed asset {}; inspect it before updating",
                target.display()
            );
        }
        if !target.exists() {
            std::fs::write(&target, bytes)
                .with_context(|| format!("write tokenizer asset {}", target.display()))?;
        }
        println!("verified {} ({digest})", asset.output);
    }
    Ok(())
}
