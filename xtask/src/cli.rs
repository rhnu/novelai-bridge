use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use novelai_bridge::{Model, ValidationPolicy};

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Assemble, inspect and send NovelAI requests through novelai-bridge"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Download pinned tokenizer assets, verify SHA-256 and store deterministic gzip files.
    FetchTokenizerAssets,
    /// Check every supported feature boundary without running tests.
    CheckFeatures,
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
    /// Encode a vibe without generating an image.
    EncodeVibe,
    /// Generate with a director character reference.
    Charref,
    /// Streaming text-to-image.
    Stream,
    /// Read the account subscription and free V5 allowance.
    Subscription,
    /// Upscale a source image using the independent image endpoint.
    Upscale,
    /// Read tag suggestions without generating images.
    Tags,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Alpha {
    Straight,
    Premultiplied,
}

impl From<Alpha> for novelai_bridge::AlphaMode {
    fn from(value: Alpha) -> Self {
        match value {
            Alpha::Straight => Self::Straight,
            Alpha::Premultiplied => Self::Premultiplied,
        }
    }
}

#[derive(Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent probe flags select different generation capabilities."
)]
pub struct RunArgs {
    /// What to build.
    #[arg(value_enum)]
    pub preset: Preset,

    /// Model identifier, for example `nai-diffusion-5-full`.
    #[arg(long, value_parser = parse_model)]
    pub model: Option<Model>,

    #[arg(long, default_value = "1girl, cat ears, looking at viewer")]
    pub prompt: String,

    #[arg(long)]
    pub negative: Option<String>,

    /// Tag suggestion language.
    #[arg(long, value_parser = ["en", "jp"], default_value = "en")]
    pub lang: String,

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

    /// Alpha representation for transparent output.
    #[arg(long, value_enum, default_value = "straight")]
    pub alpha: Alpha,

    /// Reject unsupported or out-of-range values instead of normalizing them.
    #[arg(long)]
    pub strict: bool,

    /// Source image for `i2i` / `infill` / `vibe` / `charref`. A gradient is synthesized if absent.
    #[arg(long)]
    pub image: Option<PathBuf>,

    /// Inpaint mask for `infill`. A centered rectangle is synthesized if absent.
    #[arg(long)]
    pub mask: Option<PathBuf>,

    #[arg(long, default_value_t = 1.0)]
    pub information_extracted: f32,

    /// Crop vibe encoding to the mask bounds.
    #[arg(long)]
    pub crop_to_mask: bool,

    #[arg(long, allow_hyphen_values = true)]
    pub focus_seed: Option<i64>,

    #[arg(long, allow_hyphen_values = true)]
    pub info_extract_seed: Option<i64>,

    #[arg(long, default_value_t = 0.7)]
    pub strength: f32,

    /// Use V5 Max Enhance for `i2i`, normalizing the source dimensions to the model grid.
    #[arg(long)]
    pub max_enhance: bool,

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
    #[arg(long, requires = "out")]
    pub save_inputs: bool,
}

impl RunArgs {
    pub const fn policy(&self) -> ValidationPolicy {
        if self.strict {
            ValidationPolicy::Reject
        } else {
            ValidationPolicy::Normalize
        }
    }
}

fn parse_model(raw: &str) -> Result<Model, serde_json::Error> { serde_json::from_value(raw.into()) }
