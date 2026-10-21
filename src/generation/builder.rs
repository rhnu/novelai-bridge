use crate::{Character, CharacterReference, ImageFormat, ImageSize, Model, NoiseSchedule, QualityPreset, Sampler, SamplingParams, UcPreset, VibeTransferConfig};
use super::{CharacterSet, GenerateImageRequest, GenerationCore, GuidanceParams, Img2ImgRequest, ReferenceSet};

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
                sampling: model.descriptor().defaults().sampling,
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

    /// Switches a V4-or-later model from the default anime dataset to furry mode.
    #[must_use]
    pub const fn furry_mode(mut self, enabled: bool) -> Self {
        self.request.core.furry_mode = enabled;
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
