use serde::{Deserialize, Serialize};

use crate::Model;

const QUALITY_TAGS_STANDARD: &str = ", very aesthetic, masterpiece, no text";
const QUALITY_TAGS_LIGHT: &str = ", very aesthetic, amazing quality, no text";
const UC_PRESET_HEAVY: &str = "nsfw, lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page";
const UC_PRESET_LIGHT: &str = "nsfw, lowres, artistic error, scan artifacts, worst quality, bad quality, jpeg artifacts, multiple views, very displeasing, too many watermarks, negative space, blank page";
const UC_PRESET_FURRY_FOCUS: &str = "{worst quality}, distracting watermark, unfinished, bad quality, {widescreen}, upscale, {sequence}, {{grandfathered content}}, blurred foreground, chromatic aberration, sketch, everyone, [sketch background], simple, [flat colors], ych (character), outline, multiple scenes, [[horror (theme)]], comic";
const UC_PRESET_HUMAN_FOCUS: &str = "nsfw, lowres, artistic error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, dithering, halftone, screentone, multiple views, logo, too many watermarks, negative space, blank page, @_@, mismatched pupils, glowing eyes, bad anatomy";

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
/// and [`QualityPreset::None`], gated by [`crate::PromptProfile::quality`].
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
    pub const fn for_model(model: Model) -> Self {
        model.descriptor().defaults().sampling
    }
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self::for_model(Model::default())
    }
}
