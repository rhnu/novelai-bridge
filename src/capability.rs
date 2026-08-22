//! Model capability descriptions.
//!
//! Every piece of model-specific knowledge used to assemble and validate a request lives in
//! [`ModelCapabilities`]. Request building reads capability flags instead of branching on model
//! identifiers, so supporting a new `NovelAI` model family only requires adding one registry entry.

use serde::{Deserialize, Serialize};

use crate::Model;

/// Prompt payload shape expected by a model family.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptStructure {
    /// V3 and earlier: top-level `prompt`, `uc`, `sm`, `sm_dyn`.
    Legacy,
    /// V4 and later: `v4_prompt`, `v4_negative_prompt`, `characterPrompts`.
    V4,
}

/// Reference canvas area used to scale the variety-boost sigma.
const VARIETY_REFERENCE_PIXELS: u32 = 832 * 1216;

/// Static description of what one `NovelAI` model family supports.
///
/// Obtain a value through [`Model::capabilities`].
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelCapabilities {
    /// Prompt payload shape used by this model.
    pub prompt_structure: PromptStructure,
    /// Value sent as `params_version`.
    pub params_version: u32,
    /// Factory default sampling step count.
    pub default_steps: u32,
    /// Factory default CFG scale.
    pub default_scale: f32,
    /// Maximum number of character prompts. `0` means character prompts are unsupported.
    pub max_characters: u32,
    /// Whether vibe transfer references are supported at all.
    pub supports_vibe_transfer: bool,
    /// Whether vibe references must be pre-encoded through [`crate::Client::encode_vibe`].
    pub supports_encoded_vibe: bool,
    /// Whether director-style character references are supported.
    pub supports_character_reference: bool,
    /// Whether character references remain supported for inpainting requests.
    pub supports_character_reference_inpainting: bool,
    /// Whether variety boost (`skip_cfg_above_sigma`) is supported.
    pub supports_variety_boost: bool,
    /// Whether an inpainting sibling model exists for infill requests.
    pub supports_inpainting: bool,
    /// Whether image-generation previews can be delivered through the streaming endpoint.
    pub supports_streaming: bool,
    /// Whether SMEA (`sm` / `sm_dyn`) parameters apply.
    pub supports_smea: bool,
    /// Whether `dynamic_thresholding` (decrisp) applies.
    pub supports_dynamic_thresholding: bool,
    /// Whether the model uses the V5 request extensions.
    pub uses_v5_extensions: bool,
    /// Whether an overdrawn Opus usage allowance disables free generation for this model.
    pub has_opus_usage_limit: bool,
    /// Multiplier applied by the current Anlas generation-pricing formula.
    pub anlas_cost_multiplier: f64,
    /// Whether [`crate::QualityPreset::Light`] is offered in addition to standard quality tags.
    pub supports_light_quality_preset: bool,
    /// Whether transparent-background output is offered.
    pub supports_transparent_background: bool,
    /// Base coefficient of the variety-boost sigma, when supported.
    pub variety_sigma_coefficient: Option<f32>,
    /// Prompt context limit in tokens, as reported by the `NovelAI` web client.
    pub prompt_token_limit: u32,
}

impl ModelCapabilities {
    /// Returns `true` when the model accepts character prompts.
    #[must_use]
    pub const fn supports_characters(&self) -> bool {
        self.max_characters > 0
    }

    /// Returns `true` when the model uses the V4-style prompt payload.
    #[must_use]
    pub const fn uses_v4_prompt(&self) -> bool {
        matches!(self.prompt_structure, PromptStructure::V4)
    }

    /// Returns the `skip_cfg_above_sigma` value for a canvas, or `None` when unsupported.
    ///
    /// The coefficient is model-dependent and scales with the square root of the canvas area
    /// relative to the 832x1216 reference canvas.
    #[must_use]
    pub fn variety_sigma(&self, width: u32, height: u32) -> Option<f32> {
        let coefficient = self.variety_sigma_coefficient?;
        let pixels = f64::from(width) * f64::from(height);
        let ratio = pixels / f64::from(VARIETY_REFERENCE_PIXELS);
        #[allow(clippy::cast_possible_truncation)]
        let scale = ratio.sqrt() as f32;
        Some(coefficient * scale)
    }
}

const V3: ModelCapabilities = ModelCapabilities {
    prompt_structure: PromptStructure::Legacy,
    params_version: 3,
    default_steps: 23,
    default_scale: 5.0,
    max_characters: 0,
    supports_vibe_transfer: true,
    supports_encoded_vibe: false,
    supports_character_reference: false,
    supports_character_reference_inpainting: false,
    supports_variety_boost: false,
    supports_inpainting: true,
    supports_streaming: false,
    supports_smea: true,
    supports_dynamic_thresholding: true,
    uses_v5_extensions: false,
    has_opus_usage_limit: false,
    anlas_cost_multiplier: 1.0,
    supports_light_quality_preset: false,
    supports_transparent_background: false,
    variety_sigma_coefficient: None,
    prompt_token_limit: 225,
};

const V3_FURRY: ModelCapabilities = ModelCapabilities {
    default_scale: 6.2,
    ..V3
};

const V4: ModelCapabilities = ModelCapabilities {
    prompt_structure: PromptStructure::V4,
    params_version: 3,
    default_steps: 23,
    default_scale: 5.5,
    max_characters: 6,
    supports_vibe_transfer: true,
    supports_encoded_vibe: true,
    supports_character_reference: false,
    supports_character_reference_inpainting: false,
    supports_variety_boost: true,
    supports_inpainting: true,
    supports_streaming: true,
    supports_smea: false,
    supports_dynamic_thresholding: false,
    uses_v5_extensions: false,
    has_opus_usage_limit: false,
    anlas_cost_multiplier: 1.0,
    supports_light_quality_preset: false,
    supports_transparent_background: false,
    variety_sigma_coefficient: Some(19.0),
    prompt_token_limit: 512,
};

const V45: ModelCapabilities = ModelCapabilities {
    default_scale: 5.0,
    supports_character_reference: true,
    supports_character_reference_inpainting: true,
    variety_sigma_coefficient: Some(58.0),
    ..V4
};

// V5 supports neither vibe transfer nor character reference, and drops variety boost. Factory
// defaults are 23 steps at scale 7.
const V5_FULL: ModelCapabilities = ModelCapabilities {
    params_version: 4,
    default_steps: 23,
    default_scale: 7.0,
    supports_vibe_transfer: false,
    supports_encoded_vibe: false,
    supports_character_reference: false,
    supports_variety_boost: false,
    uses_v5_extensions: true,
    has_opus_usage_limit: true,
    anlas_cost_multiplier: 1.5,
    supports_light_quality_preset: true,
    supports_transparent_background: true,
    variety_sigma_coefficient: None,
    prompt_token_limit: 1471,
    ..V45
};

const V5_CURATED: ModelCapabilities = ModelCapabilities {
    prompt_token_limit: 703,
    ..V5_FULL
};

impl Model {
    /// Returns the static capability description for this model.
    #[must_use]
    pub const fn capabilities(&self) -> ModelCapabilities {
        match self {
            Self::NaiDiffusion5Full => V5_FULL,
            Self::NaiDiffusion5Curated => V5_CURATED,
            Self::NaiDiffusion45Full | Self::NaiDiffusion45Curated => V45,
            Self::NaiDiffusion4Full | Self::NaiDiffusion4Curated => V4,
            Self::NaiDiffusion3 => V3,
            Self::NaiDiffusion3Furry => V3_FURRY,
        }
    }
}
