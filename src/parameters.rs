macro_rules! serde_string_enum {
    ($enum_type:ty, [$($variant:expr),+ $(,)?]) => {
        impl serde::Serialize for $enum_type {
            fn serialize<Serializer: serde::Serializer>(&self, serializer: Serializer) -> Result<Serializer::Ok, Serializer::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
        impl<'input> serde::Deserialize<'input> for $enum_type {
            fn deserialize<Deserializer: serde::Deserializer<'input>>(deserializer: Deserializer) -> Result<Self, Deserializer::Error> {
                let identifier = <String as serde::Deserialize>::deserialize(deserializer)?;
                [$($variant),+].into_iter().find(|variant| variant.as_str() == identifier)
                    .ok_or_else(|| serde::de::Error::custom(format!("unknown {} identifier: {identifier}", stringify!($enum_type))))
            }
        }
    };
}

use serde::{Deserialize, Serialize};
pub(crate) use serde_string_enum;

/// Supported image samplers.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Sampler {
    /// `k_euler`
    KEuler,
    /// `k_euler_ancestral`
    #[default]
    KEulerAncestral,
    /// `k_dpm_2`
    KDpm2,
    /// `k_dpm_2_ancestral`
    KDpm2Ancestral,
    /// `k_dpmpp_2m`
    KDpmpp2m,
    /// `k_dpmpp_2m_sde`
    KDpmpp2mSde,
    /// `k_dpmpp_2s_ancestral`
    KDpmpp2sAncestral,
    /// `k_dpmpp_sde`
    KDpmppSde,
    /// `ddim`
    Ddim,
    /// `ddim_v3`
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
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum NoiseSchedule {
    /// `native`
    Native,
    /// `karras`
    #[default]
    Karras,
    /// `exponential`
    Exponential,
    /// `polyexponential`
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
    #[default]
    Heavy,
    /// Light UC preset.
    Light,
    /// Furry-focused UC preset, offered by V4.5 Full and V5.
    FurryFocus,
    /// Human-focused UC preset.
    HumanFocus,
    /// Disables the selected preset; V3 retains its empty-negative fallback.
    None,
}

impl UcPreset {
    /// Provider hint identifying the selected preset; this is not a model's list index.
    #[must_use]
    pub const fn tag_hint(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Heavy => 2,
            Self::Light => 3,
            Self::HumanFocus => 4,
            Self::FurryFocus => 5,
        }
    }
}

/// Built-in quality tag presets appended to the positive prompt.
///
/// V5 exposes all three; earlier families only offer the equivalent of [`QualityPreset::Standard`]
/// and [`QualityPreset::None`], listed by [`crate::PromptProfile::quality_presets`].
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityPreset {
    /// The selected model's standard quality tags.
    #[default]
    Standard,
    /// The selected model's light quality tags. V5 only.
    Light,
    /// No quality tags.
    None,
}

impl QualityPreset {
    /// Provider hint identifying the selected quality preset.
    #[must_use]
    pub const fn tag_hint(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Standard => 1,
            Self::Light => 3,
        }
    }
}

/// Supported image stream modes.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
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
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG output.
    Png,
    /// WebP output.
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

/// Alpha representation used for transparent output.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    /// Preserve color channels independently of alpha.
    #[default]
    Straight,
    /// Multiply color channels by alpha.
    Premultiplied,
}

/// Image dimensions in pixels.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
    pub const fn pixels(&self) -> u64 { (self.width as u64) * (self.height as u64) }
}

impl Default for ImageSize {
    fn default() -> Self { Self::portrait() }
}

/// Sampling parameters.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
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

serde_string_enum!(
    Sampler,
    [
        Self::KEuler,
        Self::KEulerAncestral,
        Self::KDpm2,
        Self::KDpm2Ancestral,
        Self::KDpmpp2m,
        Self::KDpmpp2mSde,
        Self::KDpmpp2sAncestral,
        Self::KDpmppSde,
        Self::Ddim,
        Self::DdimV3
    ]
);

serde_string_enum!(
    NoiseSchedule,
    [
        Self::Native,
        Self::Karras,
        Self::Exponential,
        Self::Polyexponential
    ]
);

serde_string_enum!(StreamMode, [Self::Sse]);

serde_string_enum!(ImageFormat, [Self::Png, Self::Webp]);
pub mod bounds;
