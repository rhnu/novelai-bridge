//! Live probe harness for `novelai-bridge`.
//!
//! Two modes share one request-assembly path:
//!
//! - `shape` builds a request and prints the exact wire body without sending it. No key needed.
//! - `probe` sends it and reports the response.
//!
//! `--set <path>=<json>` rewrites a field *after* the library assembled the body, so a single
//! wire question can be A/B tested without editing the crate.
//!
//! ```text
//! cargo xtask capabilities
//! cargo xtask shape t2i --model nai-diffusion-5-full
//! cargo xtask probe i2i --image ref.png --out scripts/out
//! cargo xtask probe infill --set parameters.add_original_image=false
//! ```

mod capture;
mod presets;
mod render;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use novelai_bridge::{
    BaseUrls, BridgeError, Client, DEFAULT_TIMEOUT_MS, ImageSize, Model, ReqwestTransport,
    SecretString,
};

use capture::CaptureTransport;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Assemble, inspect and send NovelAI requests through novelai-bridge"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the model capability registry. Offline.
    Capabilities,
    /// Assemble a request and print the wire body without sending it. Offline.
    Shape(RunArgs),
    /// Assemble a request, send it, and report the response. Needs `NOVELAI_API_KEY`.
    Probe(RunArgs),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Quality {
    Standard,
    Light,
    None,
}

impl From<Quality> for novelai_bridge::QualityPreset {
    fn from(value: Quality) -> Self {
        match value {
            Quality::Standard => Self::Standard,
            Quality::Light => Self::Light,
            Quality::None => Self::None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Preset {
    /// Plain text-to-image.
    T2i,
    /// Image-to-image against a source image.
    I2i,
    /// Masked inpainting.
    Infill,
    /// Encode a vibe, then generate with it.
    Vibe,
    /// Generate with a director character reference.
    Charref,
    /// Streaming text-to-image.
    Stream,
    /// Read the account subscription and free V5 allowance.
    Subscription,
}

#[derive(Args)]
pub struct RunArgs {
    /// What to build.
    #[arg(value_enum)]
    pub preset: Preset,

    /// Model identifier, for example `nai-diffusion-5-full`.
    #[arg(long, default_value = "nai-diffusion-5-full")]
    pub model: String,

    #[arg(long, default_value = "1girl, cat ears, looking at viewer")]
    pub prompt: String,

    #[arg(long)]
    pub negative: Option<String>,

    /// Built-in quality tags. `light` is V5 only.
    #[arg(long, value_enum, default_value = "standard")]
    pub quality: Quality,

    /// Canvas as `WIDTHxHEIGHT`.
    #[arg(long, default_value = "832x1216")]
    pub size: String,

    #[arg(long)]
    pub steps: Option<u32>,

    #[arg(long)]
    pub scale: Option<f32>,

    /// Request seed. `0` lets the client pick one.
    #[arg(long, default_value_t = 0)]
    pub seed: i64,

    #[arg(long, default_value_t = 1)]
    pub samples: u32,

    /// Enable variety boost.
    #[arg(long)]
    pub variety: bool,

    /// Request a transparent background. V5 only.
    #[arg(long)]
    pub transparent: bool,

    /// Reject unsupported or out-of-range values instead of normalizing them.
    #[arg(long)]
    pub strict: bool,

    /// Source image for `i2i` / `infill` / `vibe` / `charref`. A gradient is synthesized if absent.
    #[arg(long)]
    pub image: Option<PathBuf>,

    /// Inpaint mask for `infill`. A centered rectangle is synthesized if absent.
    #[arg(long)]
    pub mask: Option<PathBuf>,

    #[arg(long, default_value_t = 0.7)]
    pub strength: f32,

    /// Rewrite a wire field after assembly, for example
    /// `--set parameters.add_original_image=false`. Repeatable.
    #[arg(long = "set", value_name = "PATH=VALUE")]
    pub overrides: Vec<String>,

    /// Remove a wire field after assembly, for example
    /// `--unset parameters.reference_information_extracted_multiple`. Repeatable.
    #[arg(long = "unset", value_name = "PATH")]
    pub removals: Vec<String>,

    /// Directory to write returned images into.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Also write the synthesized source and mask into `--out`, so results can be diffed
    /// against the exact inputs that were sent.
    #[arg(long)]
    pub save_inputs: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse().command {
        Command::Capabilities => {
            presets::print_capabilities();
            Ok(())
        }
        Command::Shape(args) => run(args, false).await,
        Command::Probe(args) => run(args, true).await,
    }
}

async fn run(args: RunArgs, live: bool) -> Result<(), Box<dyn std::error::Error>> {
    let model = parse_model(&args.model)?;
    let size = parse_size(&args.size)?;
    let overrides = args
        .overrides
        .iter()
        .map(|raw| render::parse_override(raw))
        .collect::<Result<Vec<_>, _>>()?;

    let transport = if live {
        CaptureTransport::live(
            ReqwestTransport::new(resolve_api_key()?, DEFAULT_TIMEOUT_MS)?,
            overrides,
            args.removals.clone(),
        )
    } else {
        CaptureTransport::dry_run(overrides, args.removals.clone())
    };
    let client = Client::with_transport_and_base_urls(transport, BaseUrls::default());

    match presets::execute(&client, &args, model, size).await {
        Ok(report) => {
            println!("\n{report}");
            Ok(())
        }
        // A dry run always ends in the transport's synthetic failure. Reaching it means the
        // request cleared local validation and assembly, which is the signal `shape` exists for.
        Err(BridgeError::Transport(_)) if !live => {
            println!("\ndry run: assembled successfully, nothing sent");
            Ok(())
        }
        Err(error) => Err(describe(&error).into()),
    }
}

/// Renders the full source chain, which is where the server's own reason text lives.
fn describe(error: &BridgeError) -> String {
    let mut rendered = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        rendered.push_str(&format!("\n  caused by: {inner}"));
        source = inner.source();
    }
    rendered
}

fn parse_model(raw: &str) -> Result<Model, String> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned()))
        .map_err(|_| format!("unknown model `{raw}`"))
}

fn parse_size(raw: &str) -> Result<ImageSize, String> {
    let (width, height) = raw
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("--size expects WIDTHxHEIGHT, got `{raw}`"))?;
    Ok(ImageSize {
        width: width.trim().parse().map_err(|_| "invalid width")?,
        height: height.trim().parse().map_err(|_| "invalid height")?,
    })
}

/// Reads the key from the environment, falling back to a repository-local `.env`.
fn resolve_api_key() -> Result<SecretString, String> {
    if let Ok(value) = std::env::var("NOVELAI_API_KEY")
        && !value.trim().is_empty()
    {
        return Ok(SecretString::from(value.trim().to_owned()));
    }

    let dotenv = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    let contents = std::fs::read_to_string(dotenv)
        .map_err(|_| "NOVELAI_API_KEY is not set and no .env was found".to_owned())?;

    for line in contents.lines() {
        let line = line.trim().trim_start_matches("export ").trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() == "NOVELAI_API_KEY" {
            let value = value.trim().trim_matches(['"', '\'']);
            if !value.is_empty() {
                return Ok(SecretString::from(value.to_owned()));
            }
        }
    }

    Err("NOVELAI_API_KEY is not set in the environment or .env".to_owned())
}
