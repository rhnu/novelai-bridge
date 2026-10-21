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

use capture::{CaptureTransport, RequestEdits};
use clap::Parser;
use cli::{Cli, Command, Preset, RunArgs};
use novelai_bridge::{Client, ClientConfig, ImageSize, Model, ReqwestTransport};

type TaskResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> TaskResult<()> {
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
    let model: Model = serde_json::from_value(args.model.clone().into())?;
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
        .ok_or_else(|| format!("--size expects WIDTHxHEIGHT, got `{raw}`"))?;
    Ok(ImageSize {
        width: width.trim().parse()?,
        height: height.trim().parse()?,
    })
}
