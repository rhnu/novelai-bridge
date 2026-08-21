use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::{ModelCapabilities, metadata::GeneratedImageMetadata};

const QUALITY_TAGS_STANDARD: &str = ", very aesthetic, masterpiece, no text";
const QUALITY_TAGS_LIGHT: &str = ", very aesthetic, amazing quality, no text";
const TRANSPARENT_BACKGROUND_TAG: &str = ", transparent background";
const UC_PRESET_HEAVY: &str = "nsfw, lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page";
const UC_PRESET_LIGHT: &str = "nsfw, lowres, artistic error, scan artifacts, worst quality, bad quality, jpeg artifacts, multiple views, very displeasing, too many watermarks, negative space, blank page";
const UC_PRESET_FURRY_FOCUS: &str = "{worst quality}, distracting watermark, unfinished, bad quality, {widescreen}, upscale, {sequence}, {{grandfathered content}}, blurred foreground, chromatic aberration, sketch, everyone, [sketch background], simple, [flat colors], ych (character), outline, multiple scenes, [[horror (theme)]], comic";
const UC_PRESET_HUMAN_FOCUS: &str = "nsfw, lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page, @_@, mismatched pupils, glowing eyes, bad anatomy";

/// Selects how the API key is resolved.
#[derive(Clone, Debug)]
pub enum ApiKeySource {
    /// Read the API key from the `NOVELAI_API_KEY` environment variable.
    Environment,
    /// Use an explicitly provided API key value.
    Inline { value: SecretString },
}

/// Supported `NovelAI` image models.
///
/// Inpainting siblings are not separate variants; they are derived from the base model through
/// [`Model::inpainting_id`] when a request carries an inpaint mask.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Model {
    /// `nai-diffusion-5-full`
    #[serde(rename = "nai-diffusion-5-full")]
    NaiDiffusion5Full,
    /// `nai-diffusion-5-curated`
    #[serde(rename = "nai-diffusion-5-curated")]
    NaiDiffusion5Curated,
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
    /// `nai-diffusion-furry-3`
    #[serde(rename = "nai-diffusion-furry-3")]
    NaiDiffusion3Furry,
}

impl Model {
    /// Returns the API model identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NaiDiffusion5Full => "nai-diffusion-5-full",
            Self::NaiDiffusion5Curated => "nai-diffusion-5-curated",
            Self::NaiDiffusion45Full => "nai-diffusion-4-5-full",
            Self::NaiDiffusion45Curated => "nai-diffusion-4-5-curated",
            Self::NaiDiffusion4Full => "nai-diffusion-4-full",
            Self::NaiDiffusion4Curated => "nai-diffusion-4-curated",
            Self::NaiDiffusion3 => "nai-diffusion-3",
            Self::NaiDiffusion3Furry => "nai-diffusion-furry-3",
        }
    }

    /// Returns the API identifier of the inpainting sibling used by infill requests.
    #[must_use]
    pub const fn inpainting_id(&self) -> &'static str {
        match self {
            Self::NaiDiffusion5Full => "nai-diffusion-5-full-inpainting",
            Self::NaiDiffusion5Curated => "nai-diffusion-5-curated-inpainting",
            Self::NaiDiffusion45Full => "nai-diffusion-4-5-full-inpainting",
            Self::NaiDiffusion45Curated => "nai-diffusion-4-5-curated-inpainting",
            Self::NaiDiffusion4Full => "nai-diffusion-4-full-inpainting",
            Self::NaiDiffusion4Curated => "nai-diffusion-4-curated-inpainting",
            Self::NaiDiffusion3 => "nai-diffusion-3-inpainting",
            Self::NaiDiffusion3Furry => "nai-diffusion-furry-3-inpainting",
        }
    }

    /// Returns the model key used by vibe metadata payloads.
    ///
    /// Returns `None` for model families that do not support vibe transfer at all.
    #[must_use]
    pub const fn vibe_model_key(&self) -> Option<&'static str> {
        match self {
            Self::NaiDiffusion5Full | Self::NaiDiffusion5Curated => None,
            Self::NaiDiffusion45Full => Some("v4-5full"),
            Self::NaiDiffusion45Curated => Some("v4-5curated"),
            Self::NaiDiffusion4Full => Some("v4full"),
            Self::NaiDiffusion4Curated => Some("v4curated"),
            Self::NaiDiffusion3 => Some("v3"),
            Self::NaiDiffusion3Furry => Some("v3furry"),
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
    /// `k_dpmpp_2m_sde`
    #[serde(rename = "k_dpmpp_2m_sde")]
    KDpmpp2mSde,
    /// `k_dpmpp_2s_ancestral`
    #[serde(rename = "k_dpmpp_2s_ancestral")]
    KDpmpp2sAncestral,
    /// `k_dpmpp_sde`
    #[serde(rename = "k_dpmpp_sde")]
    KDpmppSde,
    /// `ddim`
    #[serde(rename = "ddim")]
    Ddim,
    /// `ddim_v3`
    #[serde(rename = "ddim_v3")]
    DdimV3,
}

impl Sampler {
    /// Returns the API sampler identifier.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::KEuler => "k_euler",
            Self::KEulerAncestral => "k_euler_ancestral",
            Self::KDpm2 => "k_dpm_2",
            Self::KDpm2Ancestral => "k_dpm_2_ancestral",
            Self::KDpmpp2m => "k_dpmpp_2m",
            Self::KDpmpp2mSde => "k_dpmpp_2m_sde",
            Self::KDpmpp2sAncestral => "k_dpmpp_2s_ancestral",
            Self::KDpmppSde => "k_dpmpp_sde",
            Self::Ddim => "ddim",
            Self::DdimV3 => "ddim_v3",
        }
    }
}

/// Supported noise schedules.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum NoiseSchedule {
    /// `native`
    #[serde(rename = "native")]
    Native,
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
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Karras => "karras",
            Self::Exponential => "exponential",
            Self::Polyexponential => "polyexponential",
        }
    }

    /// Returns the schedule actually accepted by a model family.
    ///
    /// V4 and later reject `native`; the `NovelAI` web client sends `karras` instead.
    #[must_use]
    pub const fn resolve_for(self, capabilities: &ModelCapabilities) -> Self {
        if capabilities.uses_v4_prompt() && matches!(self, Self::Native) {
            Self::Karras
        } else {
            self
        }
    }
}

/// Built-in UC presets supported by `NovelAI`.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UcPreset {
    /// Heavy UC preset.
    Heavy,
    /// Light default UC preset.
    #[default]
    Light,
    /// Furry-focused UC preset, intended for the V3 furry model.
    FurryFocus,
    /// Human-focused UC preset.
    HumanFocus,
    /// No preset text.
    None,
}

impl UcPreset {
    /// Returns the integer value expected by the API.
    #[must_use]
    pub const fn as_api_value(&self) -> i32 {
        match self {
            Self::Heavy => 0,
            Self::Light => 1,
            Self::FurryFocus => 2,
            Self::HumanFocus => 3,
            Self::None => 4,
        }
    }

    /// Returns the preset text merged into the negative prompt.
    ///
    /// The `NovelAI` web client places this text *before* the caller's negative prompt.
    #[must_use]
    pub const fn preset_text(&self) -> &'static str {
        match self {
            Self::Heavy => UC_PRESET_HEAVY,
            Self::Light => UC_PRESET_LIGHT,
            Self::FurryFocus => UC_PRESET_FURRY_FOCUS,
            Self::HumanFocus => UC_PRESET_HUMAN_FOCUS,
            Self::None => "",
        }
    }
}

/// Built-in quality tag presets appended to the positive prompt.
///
/// V5 exposes all three; earlier families only offer the equivalent of [`QualityPreset::Standard`]
/// and [`QualityPreset::None`], gated by [`ModelCapabilities::supports_light_quality_preset`].
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityPreset {
    /// `very aesthetic, masterpiece, no text`.
    #[default]
    Standard,
    /// `very aesthetic, amazing quality, no text`. V5 only.
    Light,
    /// No quality tags.
    None,
}

impl QualityPreset {
    /// Returns the tag text appended to the positive prompt.
    #[must_use]
    pub const fn tags(&self) -> &'static str {
        match self {
            Self::Standard => QUALITY_TAGS_STANDARD,
            Self::Light => QUALITY_TAGS_LIGHT,
            Self::None => "",
        }
    }

    /// Returns the `qualityToggle` value that accompanies the merged prompt.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        !matches!(self, Self::None)
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
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
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
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
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
    #[must_use]
    pub const fn portrait() -> Self {
        Self {
            width: 832,
            height: 1216,
        }
    }

    /// Returns the default landscape canvas.
    #[must_use]
    pub const fn landscape() -> Self {
        Self {
            width: 1216,
            height: 832,
        }
    }

    /// Returns the default square canvas.
    #[must_use]
    pub const fn square() -> Self {
        Self {
            width: 1024,
            height: 1024,
        }
    }

    /// Returns the large portrait director/reference canvas.
    #[must_use]
    pub const fn large_portrait() -> Self {
        Self {
            width: 1024,
            height: 1536,
        }
    }

    /// Returns the large landscape director/reference canvas.
    #[must_use]
    pub const fn large_landscape() -> Self {
        Self {
            width: 1536,
            height: 1024,
        }
    }

    /// Returns the canvas area in pixels.
    #[must_use]
    pub const fn pixels(&self) -> u64 {
        (self.width as u64) * (self.height as u64)
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

/// Character prompt input used by V4 and later models.
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

/// Character prompts and their positioning behavior.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct CharacterSet {
    /// Character prompts sent with the request.
    pub characters: Vec<Character>,
    /// Optional override for `use_coords`.
    ///
    /// When `None`, coordinates are enabled only if an enabled character sits off-center.
    pub use_coords: Option<bool>,
}

impl CharacterSet {
    /// Creates a set from character prompts, letting the client resolve `use_coords`.
    #[must_use]
    pub const fn new(characters: Vec<Character>) -> Self {
        Self {
            characters,
            use_coords: None,
        }
    }

    /// Returns the enabled characters.
    pub fn enabled(&self) -> impl Iterator<Item = &Character> {
        self.characters.iter().filter(|character| character.enabled)
    }

    /// Returns the number of enabled characters.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.enabled().count()
    }
}

/// Character reference behavior used by director references.
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
    /// Costume reference.
    #[serde(rename = "costume")]
    Costume,
    /// Delta reference.
    #[serde(rename = "delta")]
    Delta,
}

impl CharacterReferenceType {
    /// Returns the caption value sent as `director_reference_descriptions[].caption.base_caption`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Style => "style",
            Self::CharacterAndStyle => "character&style",
            Self::Costume => "costume",
            Self::Delta => "delta",
        }
    }
}

/// Character reference input used by director references.
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

/// One vibe transfer reference.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VibeReference {
    /// Precomputed base64 vibe payload returned by [`crate::Client::encode_vibe`].
    ///
    /// The information-extracted value is baked into this payload by `encode_vibe`; generation
    /// requests carry no separate field for it.
    pub vibe_data_cache: String,
    /// Per-image reference strength.
    pub strength: f32,
}

/// Vibe transfer configuration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct VibeTransferConfig {
    /// One or more precomputed vibe payloads.
    pub references: Vec<VibeReference>,
    /// Overall vibe transfer strength.
    pub strength: f32,
}

/// Reference images attached to a generation request.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ReferenceSet {
    /// Optional vibe transfer configuration.
    pub vibe_transfer: Option<VibeTransferConfig>,
    /// Optional director-style character references.
    pub character_references: Vec<CharacterReference>,
}

impl ReferenceSet {
    /// Returns `true` when no reference of any kind is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.character_references.is_empty()
            && self
                .vibe_transfer
                .as_ref()
                .is_none_or(|vibe| vibe.references.is_empty())
    }
}

/// Image-to-image and inpainting input.
///
/// Presence of [`Img2ImgRequest::mask`] switches the request from `img2img` to `infill`, which
/// also swaps the wire model to the inpainting sibling returned by [`Model::inpainting_id`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Img2ImgRequest {
    /// Base64 payload or image data URL for the source image.
    ///
    /// The client fits this image to the requested canvas before sending it.
    pub image: String,
    /// Strength value in the `[0.01, 0.99]` range.
    pub strength: f32,
    /// Noise value in the `[0, 0.99]` range.
    pub noise: f32,
    /// Optional base64 payload or image data URL for the inpaint mask.
    pub mask: Option<String>,
    /// Whether the server applies color correction against the source image.
    pub color_correct: bool,
    /// Optional explicit `extra_noise_seed`. Defaults to the resolved request seed.
    pub extra_noise_seed: Option<i64>,
    /// Whether the server composites the untouched source outside the generated area.
    pub add_original_image: bool,
}

impl Img2ImgRequest {
    /// Creates an image-to-image request with the client defaults.
    #[must_use]
    pub fn new(image: impl Into<String>, strength: f32, noise: f32) -> Self {
        Self {
            image: image.into(),
            strength,
            noise,
            mask: None,
            color_correct: true,
            extra_noise_seed: None,
            // A live infill A/B confirms `true` leaves the area outside the mask pixel-identical
            // to the source, while `false` re-encodes the whole canvas and loses fidelity there.
            add_original_image: true,
        }
    }

    /// Returns `true` when this request targets the infill pipeline.
    #[must_use]
    pub const fn is_infill(&self) -> bool {
        self.mask.is_some()
    }
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
    /// Rejects out-of-range numeric values instead of clamping them.
    pub strict_mode: bool,
}

/// Supported `NovelAI` director tools.
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
    /// Rejects out-of-range numeric values instead of clamping them.
    pub strict_mode: bool,
}

/// Prompt, model and canvas fields shared by every generation request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationCore {
    /// Positive prompt text.
    pub prompt: String,
    /// Optional negative prompt text.
    pub negative_prompt: Option<String>,
    /// Target image model.
    pub model: Model,
    /// Output canvas size.
    pub size: ImageSize,
    /// Seed value. `0` requests a locally generated seed.
    pub seed: i64,
    /// Number of images to generate in the `[1, 4]` range.
    pub n_samples: u32,
    /// Built-in quality tags appended to the positive prompt.
    pub quality: QualityPreset,
    /// Requests a transparent background.
    ///
    /// This is a prompt-level feature: it appends a `transparent background` tag and sets the
    /// matching wire hint. Requires [`ModelCapabilities::supports_transparent_background`].
    pub transparent_background: bool,
    /// Built-in UC preset merged into the negative prompt.
    pub uc_preset: UcPreset,
}

impl GenerationCore {
    /// Creates the core fields for a prompt and model, using the canvas and seed defaults.
    #[must_use]
    pub fn new(prompt: impl Into<String>, model: Model) -> Self {
        Self {
            prompt: prompt.into(),
            model,
            ..Self::default()
        }
    }

    /// Returns the positive prompt after applying the transparency and quality tags.
    #[must_use]
    pub fn processed_prompt(&self) -> String {
        let mut prompt = self.prompt.clone();
        if self.transparent_background {
            prompt.push_str(TRANSPARENT_BACKGROUND_TAG);
        }
        prompt.push_str(self.quality.tags());
        prompt
    }

    /// Returns the negative prompt after merging the selected UC preset text.
    ///
    /// The preset text is placed before the caller's negative prompt, matching the `NovelAI`
    /// web client.
    #[must_use]
    pub fn processed_negative_prompt(&self) -> String {
        let preset = self.uc_preset.preset_text();
        let negative = self.negative_prompt.as_deref().unwrap_or_default();
        match (preset.is_empty(), negative.is_empty()) {
            (true, _) => negative.to_owned(),
            (false, true) => preset.to_owned(),
            (false, false) => format!("{preset}, {negative}"),
        }
    }
}

impl Default for GenerationCore {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            negative_prompt: None,
            model: Model::default(),
            size: ImageSize::default(),
            seed: 0,
            n_samples: 1,
            quality: QualityPreset::default(),
            transparent_background: false,
            uc_preset: UcPreset::default(),
        }
    }
}

/// Sampling parameters.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SamplingParams {
    /// Sampling step count in the `[1, 50]` range.
    pub steps: u32,
    /// CFG scale in the `[0, 10]` range.
    pub scale: f32,
    /// Sampler to use.
    pub sampler: Sampler,
    /// Noise schedule to use.
    pub noise_schedule: NoiseSchedule,
    /// CFG rescale value in the `[0, 1]` range.
    pub cfg_rescale: f32,
}

impl SamplingParams {
    /// Returns the factory defaults registered for a model.
    #[must_use]
    pub fn for_model(model: Model) -> Self {
        let capabilities = model.capabilities();
        Self {
            steps: capabilities.default_steps,
            scale: capabilities.default_scale,
            ..Self::default()
        }
    }
}

impl Default for SamplingParams {
    fn default() -> Self {
        let capabilities = Model::default().capabilities();
        Self {
            steps: capabilities.default_steps,
            scale: capabilities.default_scale,
            sampler: Sampler::default(),
            noise_schedule: NoiseSchedule::default(),
            cfg_rescale: 0.0,
        }
    }
}

/// Guidance toggles gated by model capabilities.
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuidanceParams {
    /// Enables `NovelAI`'s variety boost (`skip_cfg_above_sigma`).
    ///
    /// Requires [`ModelCapabilities::supports_variety_boost`].
    pub variety_boost: bool,
    /// Enables decrisp (`dynamic_thresholding`).
    ///
    /// Requires [`ModelCapabilities::supports_dynamic_thresholding`].
    pub decrisp: bool,
    /// Enables SMEA.
    ///
    /// Requires [`ModelCapabilities::supports_smea`].
    pub smea: bool,
    /// Enables dynamic SMEA.
    ///
    /// Requires [`ModelCapabilities::supports_smea`].
    pub smea_dyn: bool,
}

impl GuidanceParams {
    /// Returns `true` when no guidance toggle is enabled.
    #[must_use]
    pub const fn is_default(&self) -> bool {
        !self.variety_boost && !self.decrisp && !self.smea && !self.smea_dyn
    }
}

/// Output encoding options.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputParams {
    /// Optional output image format.
    pub image_format: Option<ImageFormat>,
}

/// Request used by [`crate::Client::generate`].
///
/// Fields are grouped by capability: an optional group is only accepted when
/// [`Model::capabilities`] reports support for it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GenerateImageRequest {
    /// Prompt, model, canvas and seed.
    pub core: GenerationCore,
    /// Sampling parameters.
    pub sampling: SamplingParams,
    /// Optional character prompts.
    pub characters: Option<CharacterSet>,
    /// Guidance toggles.
    pub guidance: GuidanceParams,
    /// Optional image-to-image or inpainting input.
    pub img2img: Option<Img2ImgRequest>,
    /// Optional reference images.
    pub references: Option<ReferenceSet>,
    /// Output encoding options.
    pub output: OutputParams,
    /// Rejects unsupported or out-of-range values instead of normalizing them.
    pub strict_mode: bool,
}

impl GenerateImageRequest {
    /// Starts a builder for a prompt and model.
    #[must_use]
    pub fn builder(prompt: impl Into<String>, model: Model) -> GenerateImageRequestBuilder {
        GenerateImageRequestBuilder::new(prompt, model)
    }

    /// Returns the capabilities of the selected model.
    #[must_use]
    pub const fn capabilities(&self) -> ModelCapabilities {
        self.core.model.capabilities()
    }

    /// Returns the positive prompt after applying built-in quality tags.
    #[must_use]
    pub fn processed_prompt(&self) -> String {
        self.core.processed_prompt()
    }

    /// Returns the negative prompt after merging the selected UC preset text.
    #[must_use]
    pub fn processed_negative_prompt(&self) -> String {
        self.core.processed_negative_prompt()
    }
}

/// Builder for [`GenerateImageRequest`].
#[derive(Clone, Debug)]
pub struct GenerateImageRequestBuilder {
    request: GenerateImageRequest,
}

impl GenerateImageRequestBuilder {
    /// Creates a builder seeded with the model's factory sampling defaults.
    #[must_use]
    pub fn new(prompt: impl Into<String>, model: Model) -> Self {
        Self {
            request: GenerateImageRequest {
                core: GenerationCore::new(prompt, model),
                sampling: SamplingParams::for_model(model),
                ..GenerateImageRequest::default()
            },
        }
    }

    /// Sets the negative prompt.
    #[must_use]
    pub fn negative_prompt(mut self, negative_prompt: impl Into<String>) -> Self {
        self.request.core.negative_prompt = Some(negative_prompt.into());
        self
    }

    /// Sets the output canvas size.
    #[must_use]
    pub const fn size(mut self, size: ImageSize) -> Self {
        self.request.core.size = size;
        self
    }

    /// Sets the request seed. `0` requests a locally generated seed.
    #[must_use]
    pub const fn seed(mut self, seed: i64) -> Self {
        self.request.core.seed = seed;
        self
    }

    /// Sets the number of images to generate.
    #[must_use]
    pub const fn n_samples(mut self, n_samples: u32) -> Self {
        self.request.core.n_samples = n_samples;
        self
    }

    /// Selects the built-in quality tags.
    #[must_use]
    pub const fn quality(mut self, quality: QualityPreset) -> Self {
        self.request.core.quality = quality;
        self
    }

    /// Requests a transparent background. V5 only.
    #[must_use]
    pub const fn transparent_background(mut self, enabled: bool) -> Self {
        self.request.core.transparent_background = enabled;
        self
    }

    /// Sets the UC preset.
    #[must_use]
    pub const fn uc_preset(mut self, uc_preset: UcPreset) -> Self {
        self.request.core.uc_preset = uc_preset;
        self
    }

    /// Replaces the sampling parameters.
    #[must_use]
    pub const fn sampling(mut self, sampling: SamplingParams) -> Self {
        self.request.sampling = sampling;
        self
    }

    /// Sets the sampling step count.
    #[must_use]
    pub const fn steps(mut self, steps: u32) -> Self {
        self.request.sampling.steps = steps;
        self
    }

    /// Sets the CFG scale.
    #[must_use]
    pub const fn scale(mut self, scale: f32) -> Self {
        self.request.sampling.scale = scale;
        self
    }

    /// Sets the sampler.
    #[must_use]
    pub const fn sampler(mut self, sampler: Sampler) -> Self {
        self.request.sampling.sampler = sampler;
        self
    }

    /// Sets the noise schedule.
    #[must_use]
    pub const fn noise_schedule(mut self, noise_schedule: NoiseSchedule) -> Self {
        self.request.sampling.noise_schedule = noise_schedule;
        self
    }

    /// Sets the character prompts.
    #[must_use]
    pub fn characters(mut self, characters: Vec<Character>) -> Self {
        self.request.characters = Some(CharacterSet::new(characters));
        self
    }

    /// Overrides `use_coords` for the character set.
    #[must_use]
    pub fn use_coords(mut self, use_coords: bool) -> Self {
        self.request
            .characters
            .get_or_insert_with(CharacterSet::default)
            .use_coords = Some(use_coords);
        self
    }

    /// Replaces the guidance toggles.
    #[must_use]
    pub const fn guidance(mut self, guidance: GuidanceParams) -> Self {
        self.request.guidance = guidance;
        self
    }

    /// Enables or disables variety boost.
    #[must_use]
    pub const fn variety_boost(mut self, variety_boost: bool) -> Self {
        self.request.guidance.variety_boost = variety_boost;
        self
    }

    /// Sets the image-to-image or inpainting input.
    #[must_use]
    pub fn img2img(mut self, img2img: Img2ImgRequest) -> Self {
        self.request.img2img = Some(img2img);
        self
    }

    /// Sets the vibe transfer configuration.
    #[must_use]
    pub fn vibe_transfer(mut self, vibe_transfer: VibeTransferConfig) -> Self {
        self.request
            .references
            .get_or_insert_with(ReferenceSet::default)
            .vibe_transfer = Some(vibe_transfer);
        self
    }

    /// Sets the director-style character references.
    #[must_use]
    pub fn character_references(mut self, references: Vec<CharacterReference>) -> Self {
        self.request
            .references
            .get_or_insert_with(ReferenceSet::default)
            .character_references = references;
        self
    }

    /// Sets the output image format.
    #[must_use]
    pub const fn image_format(mut self, image_format: ImageFormat) -> Self {
        self.request.output.image_format = Some(image_format);
        self
    }

    /// Enables strict validation instead of normalization.
    #[must_use]
    pub const fn strict_mode(mut self, strict_mode: bool) -> Self {
        self.request.strict_mode = strict_mode;
        self
    }

    /// Returns the assembled request.
    #[must_use]
    pub fn build(self) -> GenerateImageRequest {
        self.request
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

/// Result of a non-streaming image-generation request.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GenerateImageResult {
    /// Request-level seed actually sent to `NovelAI` after resolving an input seed of `0`.
    pub resolved_seed: i64,
    /// Images decoded from the response.
    pub images: Vec<GeneratedImage>,
}

/// Generated image payload.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GeneratedImage {
    /// Raw image bytes.
    pub bytes: Vec<u8>,
    /// Best-effort MIME type inferred from the response.
    pub mime_type: Option<String>,
    /// Best-effort inspection of metadata embedded in this specific image.
    pub metadata: GeneratedImageMetadata,
}

impl GeneratedImage {
    /// Returns the seed embedded in this image's metadata when available.
    ///
    /// This never falls back to the request-level resolved seed.
    #[must_use]
    pub fn seed(&self) -> Option<i64> {
        self.metadata.seed()
    }
}

/// Parsed stream chunk emitted by `NovelAI` image streaming.
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

/// Free V5 generation allowance reported by `/user/subscription`.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct V5UsageStatus {
    /// Whether the allowance is currently overdrawn.
    pub is_negative: bool,
    /// Remaining allowance as an integer percentage in the `[0, 100]` range.
    pub percent: u32,
    /// Seconds required to recharge one percent.
    pub seconds_until_next_percent: u64,
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
    /// Free V5 generation allowance, when the account reports one.
    pub v5_usage: Option<V5UsageStatus>,
}
