mod capture;
mod cli;
mod credentials;
mod features;
mod inputs;
mod presets;
mod render;
mod report;
mod tokenizer_assets;

use std::sync::Arc;

use anyhow::Context as _;
use capture::{CaptureTransport, RequestEdits};
use clap::Parser;
use cli::{Cli, Command, Preset, RunArgs};
use novelai_bridge::{Client, ClientConfig, ImageSize, ReqwestTransport};

type TaskResult<T> = anyhow::Result<T>;

#[tokio::main]
async fn main() -> TaskResult<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|error| {
            if std::env::var_os("RUST_LOG").is_some() {
                Err(error)
            } else {
                Ok(tracing_subscriber::EnvFilter::new("info"))
            }
        })
        .context("invalid RUST_LOG filter")?;
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
        .map_err(anyhow::Error::from_boxed)?;
    match Cli::parse().command {
        Command::FetchTokenizerAssets => tokenizer_assets::fetch().await,
        Command::CheckFeatures => features::check(),
        Command::Capabilities => {
            presets::print_capabilities();
            Ok(())
        },
        Command::Shape(args) => run(&args, false).await,
        Command::Probe(args) => run(&args, true).await,
    }
}

async fn run(args: &RunArgs, live: bool) -> TaskResult<()> {
    let model = args.model.unwrap_or_default();
    let size = parse_size(&args.size)?;
    let edits = RequestEdits::new(&args.overrides, args.removals.clone())?;
    if !live {
        return presets::shape(args, model, size, &edits);
    }
    let transport = CaptureTransport::new(ReqwestTransport::new(credentials::options()?)?, edits);
    let client = Client::new(Arc::new(transport), ClientConfig::default());
    println!("{}", presets::execute(&client, args, model, size).await?);
    Ok(())
}

fn parse_size(raw: &str) -> TaskResult<ImageSize> {
    let (width, height) = raw
        .split_once(['x', 'X'])
        .ok_or_else(|| anyhow::anyhow!("--size expects WIDTHxHEIGHT, got `{raw}`"))?;
    Ok(ImageSize {
        width: width.trim().parse().context("invalid --size width")?,
        height: height.trim().parse().context("invalid --size height")?,
    })
}
