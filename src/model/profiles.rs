use serde::{Deserialize, Serialize};

use super::Model;
use crate::{ImageSize, NoiseSchedule, QualityPreset, SamplingParams, UcPreset};

/// Prompt tokenizer used by a model family.
///
/// This selects an encoder implementation and its compatible upstream asset format. It says
/// nothing about how long a prompt may be — that is [`PromptProfile::token_limit`],
/// which varies between model variants that share one tokenizer.
///
/// The variant is always available; the encoder implementations behind it require the `tokenizer`
/// feature.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImageTokenizerKind {
    /// `OpenAI` CLIP byte-level BPE. Used by the V3 family.
    Clip,
    /// T5 `SentencePiece` Unigram. Used by the V4 and V4.5 families.
    T5,
    /// Qwen byte-level BPE driven by `tokenizer.json`. Used by the V5 family.
    Qwen,
}

impl ImageTokenizerKind {
    /// Returns a stable lowercase identifier for logs and diagnostics.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Clip => "clip",
            Self::T5 => "t5",
            Self::Qwen => "qwen",
        }
    }
}

crate::parameters::serde_string_enum!(ImageTokenizerKind, [Self::Clip, Self::T5, Self::Qwen]);

/// API identity, distinct from prompt, protocol and pricing profiles.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelIdentity {
    /// Base model.
    pub model: Model,
    /// API identifier for unmasked generation.
    pub api_id: &'static str,
}

/// A generation mode derived from the input shape, not the model's version.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GenerationMode {
    /// No source image.
    TextToImage,
    /// Source image without an inpaint mask.
    ImageToImage,
    /// Source image with an inpaint mask.
    Inpainting,
}

/// Modes accepted by an optional reference profile.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferenceModes {
    /// Text-to-image and image-to-image, but not inpainting.
    Unmasked,
    /// All generation modes, subject to the model also supporting the mode.
    All,
}

impl ReferenceModes {
    /// Whether the profile permits this mode.
    #[must_use]
    pub const fn allows(self, mode: GenerationMode) -> bool {
        matches!(self, Self::All) || !matches!(mode, GenerationMode::Inpainting)
    }
}

/// Availability of the provider's vibe input path in this bridge.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VibeInputPath {
    /// Legacy provider capability; the bridge does not implement this input path.
    ProviderOnlyLegacy,
    /// Pre-encoded caches from the bridge's encode-vibe operation.
    Encoded,
}

/// Vibe identity, input implementation and mode support form one profile.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct VibeProfile {
    /// Provider metadata key; not a transport model identifier.
    pub metadata_key: &'static str,
    /// Input path the provider uses and whether the bridge implements it.
    pub input: VibeInputPath,
    /// Modes that accept the reference.
    pub modes: ReferenceModes,
}

/// Character-reference availability exists only together with its mode constraint.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceProfile {
    /// Modes that accept character references.
    pub modes: ReferenceModes,
}

/// Presence of this profile is the inpainting capability.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InpaintingProfile {
    /// API sibling identifier required for inpainting.
    pub model_id: &'static str,
    /// Supports an independent image-to-image refinement strength within the mask operation.
    pub image_to_image: bool,
}

/// Optional capabilities carry their parameters; absent profiles mean unsupported features.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelSupport {
    /// Inpainting sibling and support.
    pub inpainting: Option<InpaintingProfile>,
    /// Availability of the streaming endpoint.
    pub streaming: bool,
    /// Provider vibe capability, including input implementation status.
    pub vibe: Option<VibeProfile>,
    /// Character-reference support and its allowed modes.
    pub character_reference: Option<ReferenceProfile>,
}

/// Inclusive numeric bounds used by request normalization.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ParameterRange<T> {
    /// Inclusive minimum.
    pub min: T,
    /// Inclusive maximum.
    pub max: T,
}

/// Canvas constraints, shared explicitly by registry entries.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CanvasConstraints {
    /// Allowed width and height range.
    pub dimension: ParameterRange<u32>,
    /// Pixel grid used by both dimensions.
    pub multiple: u32,
    /// Maximum total pixel count.
    pub max_pixels: u64,
}

/// Request bounds independent of prompt encoding and wire shape.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GenerationConstraints {
    /// Output canvas bounds.
    pub canvas: CanvasConstraints,
    /// Sampling steps.
    pub steps: ParameterRange<u32>,
    /// Number of generated images.
    pub samples: ParameterRange<u32>,
    /// CFG scale.
    pub scale: ParameterRange<f32>,
    /// CFG rescale.
    pub cfg_rescale: ParameterRange<f32>,
    /// Image-to-image strength.
    pub img2img_strength: ParameterRange<f32>,
    /// Inpainting strength.
    pub inpaint_strength: ParameterRange<f32>,
    /// Image-to-image and inpainting noise.
    pub img2img_noise: ParameterRange<f32>,
}

/// Model defaults used by request builders.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ModelDefaults {
    /// Sampling defaults.
    pub sampling: SamplingParams,
    /// Output canvas.
    pub size: ImageSize,
    /// Requested images.
    pub n_samples: u32,
    /// Quality tag selection.
    pub quality: QualityPreset,
    /// Negative-prompt preset.
    pub uc_preset: UcPreset,
}

/// Character-position geometry used by a model's structured prompt.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CharacterPositionProfile {
    /// V4/V4.5 positions are projected onto the centers of a discrete 5x5 grid.
    Grid5x5,
    /// V5 positions retain any normalized point on the canvas.
    Freeform,
}

impl CharacterPositionProfile {
    /// Resolves one already-clamped coordinate according to this model's geometry.
    #[must_use]
    pub fn resolve_coordinate(self, value: f32) -> f32 {
        match self {
            Self::Grid5x5 => {
                const GRID_COORDS: [f32; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];
                let mut best = GRID_COORDS[0];
                let mut best_distance = (best - value).abs();
                for candidate in GRID_COORDS.into_iter().skip(1) {
                    let distance = (candidate - value).abs();
                    if distance < best_distance {
                        best = candidate;
                        best_distance = distance;
                    }
                }
                best
            },
            Self::Freeform => value,
        }
    }
}

/// Character prompt availability, count limit and position geometry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CharacterPromptProfile {
    /// Maximum enabled characters.
    pub max_characters: u32,
    /// Model-specific character-position geometry.
    pub positioning: CharacterPositionProfile,
}

/// A model's supported quality selection and its effective prompt suffix.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct QualityPresetDefinition {
    /// Typed selection used by requests and wire hints.
    pub preset: QualityPreset,
    /// Appended without delimiters when the preset is selected.
    pub suffix: &'static str,
}

/// A model's supported undesired-content selection and its negative-prompt prefix.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UcPresetDefinition {
    /// Typed selection used by requests and wire hints.
    pub preset: UcPreset,
    /// Prepended to the negative prompt before contextual suppression.
    pub prefix: &'static str,
}

/// Conditioning support, independent from its wire representation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PromptProfile {
    /// Encoder and compatible asset format.
    pub tokenizer: ImageTokenizerKind,
    /// Context limit per conditioning field, not vocabulary size.
    pub token_limit: u32,
    /// Optional character prompting with its limit.
    pub characters: Option<CharacterPromptProfile>,
    /// Supported quality presets and their model-specific suffixes.
    pub quality_presets: &'static [QualityPresetDefinition],
    /// Supported UC presets and their model-specific prefixes.
    pub uc_presets: &'static [UcPresetDefinition],
    /// Adds `nsfw` to a nonempty selected UC preset unless the positive prompt includes it.
    pub suppress_nsfw: bool,
    /// Transparent-background tag support.
    pub transparent_background: bool,
    /// Anime/furry prompt dataset switching support.
    pub furry_mode: bool,
}

impl PromptProfile {
    /// Finds a quality selection supported by this model.
    #[must_use]
    pub fn quality_preset(
        &self,
        preset: QualityPreset,
    ) -> Option<&'static QualityPresetDefinition> {
        self.quality_presets
            .iter()
            .find(|definition| definition.preset == preset)
    }

    /// Finds an undesired-content selection supported by this model.
    #[must_use]
    pub fn uc_preset(&self, preset: UcPreset) -> Option<&'static UcPresetDefinition> {
        self.uc_presets
            .iter()
            .find(|definition| definition.preset == preset)
    }

    /// Maximum enabled characters, or zero when unsupported.
    #[must_use]
    pub const fn max_characters(&self) -> u32 {
        match self.characters {
            Some(profile) => profile.max_characters,
            None => 0,
        }
    }
}

/// Supported schedules and their fixed fallback, independent from prompt structure.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoiseScheduleSupport {
    /// Native and explicit schedules are all supported.
    NativeAndExplicit,
    /// Native is replaced by Karras; explicit schedules are preserved.
    ExplicitOnly,
}

impl NoiseScheduleSupport {
    /// Whether a schedule is accepted unchanged.
    #[must_use]
    pub const fn supports(self, schedule: NoiseSchedule) -> bool {
        !matches!(schedule, NoiseSchedule::Native) || matches!(self, Self::NativeAndExplicit)
    }

    /// Applies this sampling profile's fallback.
    #[must_use]
    pub const fn resolve(self, schedule: NoiseSchedule) -> NoiseSchedule {
        if self.supports(schedule) {
            schedule
        } else {
            NoiseSchedule::Karras
        }
    }
}

/// Optional variety boost parameters.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct VarietyBoostProfile {
    /// Base sigma coefficient at 832 by 1216 pixels.
    pub sigma_coefficient: f32,
}

impl VarietyBoostProfile {
    /// Scales sigma with the square root of canvas area.
    #[must_use]
    pub fn sigma(self, width: u32, height: u32) -> f32 {
        let ratio = (f64::from(width) * f64::from(height)) / f64::from(832 * 1216);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "The public sigma coefficient and output use provider f32 precision"
        )]
        let scale = ratio.sqrt() as f32;
        self.sigma_coefficient * scale
    }
}

/// Sampling support, separate from protocol version and prompt tokenizer.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SamplingProfile {
    /// Noise-schedule support and fallback.
    pub noise_schedules: NoiseScheduleSupport,
    /// SMEA and dynamic SMEA parameters.
    pub smea: bool,
    /// Dynamic thresholding (decrisp).
    pub dynamic_thresholding: bool,
    /// Optional variety boost with its coefficient.
    pub variety: Option<VarietyBoostProfile>,
}

impl SamplingProfile {
    /// Returns the effective schedule.
    #[must_use]
    pub const fn resolve_noise_schedule(&self, schedule: NoiseSchedule) -> NoiseSchedule {
        self.noise_schedules.resolve(schedule)
    }

    /// Returns variety sigma or `None` if unsupported.
    #[must_use]
    pub fn variety_sigma(&self, width: u32, height: u32) -> Option<f32> {
        self.variety.map(|profile| profile.sigma(width, height))
    }
}

/// Prompt representation on the wire.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PromptStructure {
    /// Flat prompt and negative-prompt fields.
    Legacy,
    /// Structured base and character captions.
    V4,
}

/// Correlated protocol version and extension fields are selected together.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WireProfile {
    /// Version 3 parameters with flat conditioning.
    LegacyV3,
    /// Version 3 parameters with structured conditioning.
    StructuredV4,
    /// Version 4 parameters with structured conditioning and V5 extensions.
    StructuredV5,
}

impl WireProfile {
    /// Protocol parameter version.
    #[must_use]
    pub const fn params_version(self) -> u32 {
        match self {
            Self::LegacyV3 | Self::StructuredV4 => 3,
            Self::StructuredV5 => 4,
        }
    }

    /// Conditioning representation.
    #[must_use]
    pub const fn prompt_structure(self) -> PromptStructure {
        match self {
            Self::LegacyV3 => PromptStructure::Legacy,
            _ => PromptStructure::V4,
        }
    }

    /// Whether structured captions are used.
    #[must_use]
    pub const fn uses_structured_prompt(self) -> bool { !matches!(self, Self::LegacyV3) }

    /// Whether V5 protocol extension fields are emitted.
    #[must_use]
    pub const fn uses_v5_extensions(self) -> bool { matches!(self, Self::StructuredV5) }
}

/// Free generation allowance policy, separate from pricing multipliers.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpusAllowance {
    /// Ordinary Opus first-image eligibility.
    Standard,
    /// Free generation additionally depends on a non-overdrawn usage allowance.
    Metered,
}

/// Model-specific parameters for the shared pricing formula.
#[derive(Copy, Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PricingProfile {
    /// Model price multiplier.
    pub multiplier: f64,
    /// Opus usage policy.
    pub opus_allowance: OpusAllowance,
}
