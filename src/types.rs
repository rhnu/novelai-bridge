use secrecy::SecretString;
use serde::{Deserialize, Serialize};

const DEFAULT_STEPS: u32 = 23;
const DEFAULT_SCALE: f32 = 5.0;
const QUALITY_TAGS: &str = ", very aesthetic, masterpiece, no text";
const UC_PRESET_HEAVY: &str = ", lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page, ";
const UC_PRESET_LIGHT: &str = ", lowres, artistic error, scan artifacts, worst quality, bad quality, jpeg artifacts, multiple views, very displeasing, too many watermarks, negative space, blank page, ";
const UC_PRESET_FURRY_FOCUS: &str = ", {worst quality}, distracting watermark, unfinished, bad quality, {widescreen}, upscale, {sequence}, {{grandfathered content}}, blurred foreground, chromatic aberration, sketch, everyone, [sketch background], simple, [flat colors], ych (character), outline, multiple scenes, [[horror (theme)]], comic, ";
const UC_PRESET_HUMAN_FOCUS: &str = ", lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page, @_@, mismatched pupils, glowing eyes, bad anatomy, ";

/// Selects how the API key is resolved.
#[derive(Clone, Debug)]
pub enum ApiKeySource {
    /// Read the API key from the `NOVELAI_API_KEY` environment variable.
    Environment,
    /// Use an explicitly provided API key value.
    Inline { value: SecretString },
}

/// Supported NovelAI image models.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Model {
    /// `nai-diffusion-4-5-full`
    #[serde(rename = "nai-diffusion-4-5-full")]
    #[default]
    NaiDiffusion45Full,
    /// `nai-diffusion-4-5-curated`
    #[serde(rename = "nai-diffusion-4-5-curated")]
    NaiDiffusion45Curated,
    /// `nai-diffusion-4-full`
    #[serde(rename = "nai-diffusion-4-full")]
    NaiDiffusion4Full,
    /// `nai-diffusion-4-curated`
    #[serde(rename = "nai-diffusion-4-curated")]
    NaiDiffusion4Curated,
    /// `nai-diffusion-3`
    #[serde(rename = "nai-diffusion-3")]
    NaiDiffusion3,
    /// `nai-diffusion-3-furry`
    #[serde(rename = "nai-diffusion-3-furry")]
    NaiDiffusion3Furry,
}

impl Model {
    /// Returns the API model identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NaiDiffusion45Full => "nai-diffusion-4-5-full",
            Self::NaiDiffusion45Curated => "nai-diffusion-4-5-curated",
            Self::NaiDiffusion4Full => "nai-diffusion-4-full",
            Self::NaiDiffusion4Curated => "nai-diffusion-4-curated",
            Self::NaiDiffusion3 => "nai-diffusion-3",
            Self::NaiDiffusion3Furry => "nai-diffusion-3-furry",
        }
    }

    /// Returns `true` when the model uses the V4/V4.5 request shape.
    pub fn is_v4(&self) -> bool {
        matches!(
            self,
            Self::NaiDiffusion45Full
                | Self::NaiDiffusion45Curated
                | Self::NaiDiffusion4Full
                | Self::NaiDiffusion4Curated
        )
    }

    /// Returns `true` when the model is a V4.5 variant.
    pub fn is_v45(&self) -> bool {
        matches!(self, Self::NaiDiffusion45Full | Self::NaiDiffusion45Curated)
    }

    /// Returns the model key used by vibe metadata payloads.
    pub fn vibe_model_key(&self) -> &'static str {
        match self {
            Self::NaiDiffusion45Full => "v4-5full",
            Self::NaiDiffusion45Curated => "v4-5curated",
            Self::NaiDiffusion4Full => "v4full",
            Self::NaiDiffusion4Curated => "v4curated",
            Self::NaiDiffusion3 => "v3",
            Self::NaiDiffusion3Furry => "v3furry",
        }
    }
}

/// Supported image samplers.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Sampler {
    /// `k_euler`
    #[serde(rename = "k_euler")]
    KEuler,
    /// `k_euler_ancestral`
    #[serde(rename = "k_euler_ancestral")]
    #[default]
    KEulerAncestral,
    /// `k_dpm_2`
    #[serde(rename = "k_dpm_2")]
    KDpm2,
    /// `k_dpm_2_ancestral`
    #[serde(rename = "k_dpm_2_ancestral")]
    KDpm2Ancestral,
    /// `k_dpmpp_2m`
    #[serde(rename = "k_dpmpp_2m")]
    KDpmpp2m,
    /// `k_dpmpp_2s_ancestral`
    #[serde(rename = "k_dpmpp_2s_ancestral")]
    KDpmpp2sAncestral,
    /// `k_dpmpp_sde`
    #[serde(rename = "k_dpmpp_sde")]
    KDpmppSde,
    /// `ddim`
    #[serde(rename = "ddim")]
    Ddim,
}

impl Sampler {
    /// Returns the API sampler identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KEuler => "k_euler",
            Self::KEulerAncestral => "k_euler_ancestral",
            Self::KDpm2 => "k_dpm_2",
            Self::KDpm2Ancestral => "k_dpm_2_ancestral",
            Self::KDpmpp2m => "k_dpmpp_2m",
            Self::KDpmpp2sAncestral => "k_dpmpp_2s_ancestral",
            Self::KDpmppSde => "k_dpmpp_sde",
            Self::Ddim => "ddim",
        }
    }
}

/// Supported noise schedules.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum NoiseSchedule {
    /// `karras`
    #[serde(rename = "karras")]
    #[default]
    Karras,
    /// `exponential`
    #[serde(rename = "exponential")]
    Exponential,
    /// `polyexponential`
    #[serde(rename = "polyexponential")]
    Polyexponential,
}

impl NoiseSchedule {
    /// Returns the API noise schedule identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Karras => "karras",
            Self::Exponential => "exponential",
            Self::Polyexponential => "polyexponential",
        }
    }
}

/// Built-in UC presets supported by NovelAI.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UcPreset {
    /// Heavy default UC preset.
    Heavy,
    /// Light default UC preset.
    #[default]
    Light,
    /// Furry-focused UC preset.
    FurryFocus,
    /// Human-focused UC preset.
    HumanFocus,
    /// No preset text.
    None,
}

impl UcPreset {
    /// Returns the integer value expected by the API.
    pub fn as_api_value(&self) -> i32 {
        match self {
            Self::Heavy => 0,
            Self::Light => 1,
            Self::FurryFocus => 2,
            Self::HumanFocus => 3,
            Self::None => 4,
        }
    }

    /// Returns the preset text appended to the negative prompt.
    pub fn preset_text(&self) -> &'static str {
        match self {
            Self::Heavy => UC_PRESET_HEAVY,
            Self::Light => UC_PRESET_LIGHT,
            Self::FurryFocus => UC_PRESET_FURRY_FOCUS,
            Self::HumanFocus => UC_PRESET_HUMAN_FOCUS,
            Self::None => "",
        }
    }
}

/// Supported image stream modes.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StreamMode {
    /// Server-sent events.
    #[default]
    Sse,
}

impl StreamMode {
    /// Returns the API stream identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sse => "sse",
        }
    }
}

/// Supported generated image output formats.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG output.
    #[serde(rename = "png")]
    Png,
    /// WebP output.
    #[serde(rename = "webp")]
    Webp,
}

impl ImageFormat {
    /// Returns the API output format identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

/// Image dimensions in pixels.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl ImageSize {
    /// Returns the default portrait canvas.
    pub fn portrait() -> Self {
        Self {
            width: 832,
            height: 1216,
        }
    }

    /// Returns the default landscape canvas.
    pub fn landscape() -> Self {
        Self {
            width: 1216,
            height: 832,
        }
    }

    /// Returns the default square canvas.
    pub fn square() -> Self {
        Self {
            width: 1024,
            height: 1024,
        }
    }

    /// Returns the large portrait director/reference canvas.
    pub fn large_portrait() -> Self {
        Self {
            width: 1024,
            height: 1536,
        }
    }

    /// Returns the large landscape director/reference canvas.
    pub fn large_landscape() -> Self {
        Self {
            width: 1536,
            height: 1024,
        }
    }
}

impl Default for ImageSize {
    fn default() -> Self {
        Self::portrait()
    }
}

/// Normalized character position in the `[0, 1]` range.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterPosition {
    /// Horizontal position in the `[0, 1]` range.
    pub x: f32,
    /// Vertical position in the `[0, 1]` range.
    pub y: f32,
}

impl Default for CharacterPosition {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Character prompt input used by V4/V4.5 models.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Character {
    /// Positive prompt for the character.
    pub prompt: String,
    /// Optional negative prompt for the character.
    pub negative_prompt: Option<String>,
    /// Desired character position.
    pub position: CharacterPosition,
    /// Whether this character is enabled.
    pub enabled: bool,
}

/// Character reference behavior used by V4.5 director references.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum CharacterReferenceType {
    /// Character-only reference.
    #[serde(rename = "character")]
    Character,
    /// Style-only reference.
    #[serde(rename = "style")]
    Style,
    /// Combined character and style reference.
    #[serde(rename = "character&style")]
    CharacterAndStyle,
}

/// Character reference input used by V4.5 models.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterReference {
    /// Base64 payload or image data URL for the reference image.
    pub image: String,
    /// How the reference should be interpreted.
    pub reference_type: CharacterReferenceType,
    /// Fidelity value used to derive secondary strength.
    pub fidelity: f32,
    /// Overall reference strength.
    pub strength: f32,
}

/// Image-to-image input.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Img2ImgRequest {
    /// Base64 payload or image data URL for the source image.
    pub image: String,
    /// Strength value in the `(0, 1)` range.
    pub strength: f32,
    /// Noise value in the `[0, 0.99]` range.
    pub noise: f32,
    /// Optional base64 payload or image data URL for the inpaint mask.
    pub mask: Option<String>,
}

/// One controlnet-style reference input.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ControlNetInput {
    /// Base64 payload or image data URL for the source image.
    pub image: String,
    /// Information-extracted value used for vibe encoding.
    pub info_extracted: f32,
    /// Per-image reference strength.
    pub strength: f32,
    /// Model used when encoding the vibe payload.
    pub model: Model,
    /// Optional precomputed base64 vibe payload.
    pub vibe_data_cache: Option<String>,
}

/// Controlnet-style configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ControlNetConfig {
    /// One or more controlnet images.
    pub images: Vec<ControlNetInput>,
    /// Overall controlnet strength.
    pub strength: f32,
}

/// Request used by [`crate::Client::encode_vibe`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncodeVibeRequest {
    /// Base64 payload or image data URL for the source image.
    pub image: String,
    /// Information-extracted value in the `[0.01, 1.0]` range.
    pub information_extracted: f32,
    /// Target model for the encoded vibe payload.
    pub model: Model,
}

/// Supported NovelAI director tools.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DirectorTool {
    /// Line art extraction.
    Lineart,
    /// Sketch conversion.
    Sketch,
    /// Background removal.
    BgRemoval,
    /// Emotion transfer.
    Emotion,
    /// Declutter cleanup.
    Declutter,
    /// Colorization.
    Colorize,
}

/// Request used by [`crate::Client::run_director_tool`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunDirectorToolRequest {
    /// Director tool to run.
    pub tool: DirectorTool,
    /// Base64 payload or image data URL for the source image.
    pub image: String,
    /// Optional prompt for tools that accept it.
    pub prompt: Option<String>,
    /// Optional defry value for tools that accept it.
    pub defry: Option<u8>,
}

/// Request used by [`crate::Client::generate`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenerateImageRequest {
    /// Positive prompt text.
    pub prompt: String,
    /// Target image model.
    pub model: Model,
    /// Output canvas size.
    pub size: ImageSize,
    /// Optional negative prompt text.
    pub negative_prompt: Option<String>,
    /// Whether quality tags are appended to the positive prompt.
    pub quality: bool,
    /// Built-in UC preset appended to the negative prompt.
    pub uc_preset: UcPreset,
    /// Sampling step count in the `[1, 50]` range.
    pub steps: u32,
    /// CFG scale in the `[0, 10]` range.
    pub scale: f32,
    /// Sampler to use.
    pub sampler: Sampler,
    /// Noise schedule to use.
    pub noise_schedule: NoiseSchedule,
    /// Seed value. `0` requests a locally generated seed.
    pub seed: i64,
    /// Number of images to generate in the `[1, 4]` range.
    pub n_samples: u32,
    /// CFG rescale value in the `[0, 1]` range.
    pub cfg_rescale: f32,
    /// Enables NovelAI's variety boost behavior.
    pub variety_boost: bool,
    /// Optional image-to-image configuration.
    pub i2i: Option<Img2ImgRequest>,
    /// Optional controlnet-style configuration.
    pub controlnet: Option<ControlNetConfig>,
    /// Optional V4.5 character references.
    pub character_references: Option<Vec<CharacterReference>>,
    /// Optional V4/V4.5 character prompts.
    pub characters: Option<Vec<Character>>,
    /// Optional override for `use_coords`.
    pub use_coords: Option<bool>,
    /// Optional output image format.
    pub image_format: Option<ImageFormat>,
}

impl Default for GenerateImageRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            model: Model::default(),
            size: ImageSize::default(),
            negative_prompt: None,
            quality: true,
            uc_preset: UcPreset::default(),
            steps: DEFAULT_STEPS,
            scale: DEFAULT_SCALE,
            sampler: Sampler::default(),
            noise_schedule: NoiseSchedule::default(),
            seed: 0,
            n_samples: 1,
            cfg_rescale: 0.0,
            variety_boost: false,
            i2i: None,
            controlnet: None,
            character_references: None,
            characters: None,
            use_coords: None,
            image_format: None,
        }
    }
}

impl GenerateImageRequest {
    /// Returns the positive prompt after applying built-in quality tags.
    pub fn processed_prompt(&self) -> String {
        if self.quality {
            format!("{}{}", self.prompt, QUALITY_TAGS)
        } else {
            self.prompt.clone()
        }
    }

    /// Returns the negative prompt after applying the selected UC preset text.
    pub fn processed_negative_prompt(&self) -> String {
        format!(
            "{}{}",
            self.negative_prompt.clone().unwrap_or_default(),
            self.uc_preset.preset_text()
        )
    }
}

/// Request used by [`crate::Client::generate_stream`].
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GenerateImageStreamRequest {
    /// Base image-generation request.
    pub base: GenerateImageRequest,
    /// Stream delivery mode.
    pub stream: StreamMode,
}

/// Generated image payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GeneratedImage {
    /// Raw image bytes.
    pub bytes: Vec<u8>,
    /// Best-effort MIME type inferred from the response.
    pub mime_type: Option<String>,
    /// Seed associated with the generated image when known.
    pub seed: Option<i64>,
}

/// Parsed stream chunk emitted by NovelAI image streaming.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageStreamChunk {
    /// Stream event type.
    pub event_type: String,
    /// Sample index.
    pub samp_ix: u32,
    /// Optional step index.
    pub step_ix: Option<u32>,
    /// Generation identifier.
    pub gen_id: u32,
    /// Optional sigma value.
    pub sigma: Option<f32>,
    /// Base64-encoded image payload fragment.
    pub image: String,
}

impl ImageStreamChunk {
    /// Parses a single SSE line into an [`ImageStreamChunk`].
    pub fn from_sse_line(line: &str) -> Result<Self, serde_json::Error> {
        let payload = line
            .strip_prefix("data:")
            .map(str::trim_start)
            .unwrap_or(line);
        serde_json::from_str(payload)
    }
}

/// Subscription summary returned by [`crate::Client::get_subscription`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionInfo {
    /// Combined fixed and purchased Anlas balance.
    pub anlas_balance: i64,
    /// Whether the account is effectively on Opus tier.
    pub is_opus: bool,
    /// Numeric tier value.
    pub tier: i32,
    /// Lowercase tier name.
    pub tier_name: String,
    /// Optional expiration timestamp in milliseconds since the Unix epoch.
    pub expires_at_ms: Option<u64>,
}
