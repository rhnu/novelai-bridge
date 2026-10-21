use serde::{Deserialize, Serialize};

use super::{CharacterSet, GenerateImageRequestBuilder, GenerationInput, ReferenceSet};
use crate::{
    AlphaMode, GenerationMode, ImageFormat, ImageSize, Model, ModelDescriptor, QualityPreset,
    SamplingParams, StreamMode, UcPreset, ValidationPolicy,
};

/// Prompt, model and canvas fields shared by every generation request.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GenerationCore {
    /// Positive prompt text.
    pub prompt: String,
    /// Optional negative prompt text.
    pub negative_prompt: Option<String>,
    /// Target image model.
    pub model: Model,
    /// Generation canvas, or source canvas when Max Enhance is enabled.
    pub size: ImageSize,
    /// Seed value. `0` requests a locally generated seed.
    pub seed: i64,
    /// Number of images to generate, capped at 8, 6 or 4 by effective output area.
    pub n_samples: u32,
    /// Built-in quality tags appended to the positive prompt.
    pub quality: QualityPreset,
    /// Requests a transparent background.
    ///
    /// This is a prompt-level feature: it appends a `transparent background` tag and sets the
    /// matching wire hint. Requires [`crate::PromptProfile::transparent_background`].
    pub transparent_background: bool,
    /// Switches supported V4-and-later models from the default anime dataset to furry mode.
    ///
    /// This is a prompt-level feature that prepends a `fur dataset` tag. Requires
    /// [`crate::PromptProfile::furry_mode`].
    #[serde(default)]
    pub furry_mode: bool,
    /// Built-in UC preset merged into the negative prompt.
    pub uc_preset: UcPreset,
}

impl std::fmt::Debug for GenerationCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerationCore")
            .field("model", &self.model)
            .field("size", &self.size)
            .field("seed", &self.seed)
            .field("n_samples", &self.n_samples)
            .field("quality", &self.quality)
            .field("transparent_background", &self.transparent_background)
            .field("furry_mode", &self.furry_mode)
            .field("uc_preset", &self.uc_preset)
            .finish_non_exhaustive()
    }
}

impl GenerationCore {
    /// Creates the core fields for a prompt and model, using the canvas and seed defaults.
    #[must_use]
    pub fn new(prompt: impl Into<String>, model: Model) -> Self {
        let defaults = model.descriptor().defaults();
        Self {
            prompt: prompt.into(),
            negative_prompt: None,
            model,
            size: defaults.size,
            seed: 0,
            n_samples: defaults.n_samples,
            quality: defaults.quality,
            transparent_background: false,
            furry_mode: false,
            uc_preset: defaults.uc_preset,
        }
    }
}

impl Default for GenerationCore {
    fn default() -> Self { Self::new(String::new(), Model::default()) }
}

/// Guidance toggles gated by model capabilities.
#[expect(
    clippy::struct_excessive_bools,
    reason = "Provider guidance options are independent capability switches"
)]
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GuidanceParams {
    /// Enables `NovelAI`'s variety boost (`skip_cfg_above_sigma`).
    ///
    /// Requires [`crate::SamplingProfile::variety`].
    pub variety_boost: bool,
    /// Enables decrisp (`dynamic_thresholding`).
    ///
    /// Requires [`crate::SamplingProfile::dynamic_thresholding`].
    pub decrisp: bool,
    /// Enables SMEA.
    ///
    /// Requires [`crate::SamplingProfile::smea`].
    pub smea: bool,
    /// Enables dynamic SMEA.
    ///
    /// Requires [`crate::SamplingProfile::smea`].
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
#[serde(deny_unknown_fields)]
pub struct OutputParams {
    /// Optional output image format.
    pub image_format: Option<ImageFormat>,
    /// Alpha representation for supported transparent generation.
    #[serde(default)]
    pub alpha_mode: AlphaMode,
}

/// Editable input normalized into an effective generation plan.
///
/// Fields are grouped by capability: an optional group is only accepted when
/// [`Model::descriptor`] reports support for it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GenerateImageRequest {
    /// Prompt, model, canvas and seed.
    pub core: GenerationCore,
    /// Sampling parameters.
    pub sampling: SamplingParams,
    /// Optional character prompts.
    pub characters: Option<CharacterSet>,
    /// Guidance toggles.
    pub guidance: GuidanceParams,
    /// Operation and its mode-specific input.
    pub operation: GenerationInput,
    /// Optional reference images.
    pub references: Option<ReferenceSet>,
    /// Output encoding options.
    pub output: OutputParams,
    /// Handling of unsupported and out-of-range editable values.
    pub policy: ValidationPolicy,
}

impl GenerateImageRequest {
    /// Starts a builder for a prompt and model.
    #[must_use]
    pub fn builder(prompt: impl Into<String>, model: Model) -> GenerateImageRequestBuilder {
        GenerateImageRequestBuilder::new(prompt, model)
    }

    /// Returns the read-only descriptor of the selected model.
    #[must_use]
    pub const fn descriptor(&self) -> &'static ModelDescriptor { self.core.model.descriptor() }

    /// Returns the generation mode represented by the raw request shape.
    #[must_use]
    pub const fn mode(&self) -> GenerationMode { self.operation.mode() }
}

/// Generation input and delivery options for incremental image output.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GenerateImageStreamRequest {
    /// Base image-generation request.
    pub base: GenerateImageRequest,
    /// Stream delivery mode.
    pub stream: StreamMode,
}

impl GenerateImageStreamRequest {
    /// Returns the underlying ordinary generation request.
    #[must_use]
    pub const fn base(&self) -> &GenerateImageRequest { &self.base }
}

impl AsRef<GenerateImageRequest> for GenerateImageStreamRequest {
    fn as_ref(&self) -> &GenerateImageRequest { self.base() }
}

impl Default for GenerateImageRequest {
    fn default() -> Self {
        let model = Model::default();
        Self {
            core: GenerationCore::default(),
            sampling: model.descriptor().defaults().sampling,
            characters: None,
            guidance: GuidanceParams::default(),
            operation: GenerationInput::TextToImage,
            references: None,
            output: OutputParams::default(),
            policy: ValidationPolicy::Normalize,
        }
    }
}
