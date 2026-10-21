use std::{path::Path, process::Command};

use anyhow::Context as _;

use crate::TaskResult;

pub fn check() -> TaskResult<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask must be a workspace member")?;
    for features in [
        "",
        "png-metadata",
        "image-processing",
        "tokenizer",
        "bundled-assets",
        "generation",
        "generation,tokenizer",
        "client",
        "reqwest-transport",
        "streaming",
        "reqwest-streaming",
        "credit-observer",
    ] {
        let mut command = Command::new("cargo");
        command.current_dir(root).args([
            "check",
            "--all-targets",
            "--package",
            "novelai-bridge",
            "--no-default-features",
            "--locked",
        ]);
        if !features.is_empty() {
            command.args(["--features", features]);
        }
        println!(
            "Checking features: {}",
            if features.is_empty() {
                "(core)"
            } else {
                features
            }
        );
        if !command
            .status()
            .context("start Cargo feature compilation")?
            .success()
        {
            anyhow::bail!("feature check failed: {features}");
        }
    }
    Ok(())
}
