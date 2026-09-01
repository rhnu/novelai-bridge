use serde::{Deserialize, Serialize};

use crate::{
    GenerationMode, ImageFormat, ImageSize, Model, ModelDescriptor, NoiseSchedule, QualityPreset,
    Sampler, SamplingParams, StreamMode, UcPreset, VibeTransferConfig,
    metadata::GeneratedImageMetadata,
};

const TRANSPARENT_BACKGROUND_TAG: &str = ", transparent background";

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
    /// Validated reference image.
    pub image: crate::EncodedImage,
    /// How the reference should be interpreted.
    pub reference_type: CharacterReferenceType,
    /// Fidelity value used to derive secondary strength.
    pub fidelity: f32,
    /// Overall reference strength.
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
    /// Validated source image.
    ///
    /// The client fits this image to the requested canvas before sending it.
    pub image: crate::EncodedImage,
    /// Strength value in the `[0.01, 0.99]` range.
    pub strength: f32,
    /// Noise value in the `[0, 0.99]` range.
    pub noise: f32,
    /// Optional validated inpaint mask.
    pub mask: Option<crate::EncodedImage>,
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
    pub const fn new(image: crate::EncodedImage, strength: f32, noise: f32) -> Self {
        Self {
            image,
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
    /// matching wire hint. Requires [`crate::PromptProfile::transparent_background`].
    pub transparent_background: bool,
    /// Built-in UC preset merged into the negative prompt.
    pub uc_preset: UcPreset,
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
            uc_preset: defaults.uc_preset,
        }
    }

    /// Assembles the positive prompt from every contributing source.
    fn assemble_prompt(&self) -> String {
        let mut prompt = self.prompt.clone();
        if self.transparent_background {
            prompt.push_str(TRANSPARENT_BACKGROUND_TAG);
        }
        prompt.push_str(self.quality.tags());
        prompt
    }

    /// Assembles the negative prompt from every contributing source.
    ///
    /// The UC preset text is placed before the caller's negative prompt, matching the `NovelAI`
    /// web client.
    fn assemble_negative_prompt(&self) -> String {
        let preset = self.uc_preset.preset_text();
        let negative = self.negative_prompt.as_deref().unwrap_or_default();
        match (preset.is_empty(), negative.is_empty()) {
            (true, _) => negative.to_owned(),
            (false, true) => preset.to_owned(),
            (false, false) => format!("{preset}, {negative}"),
        }
    }
}

/// Final conditioning text a request will actually send, per field.
///
/// A prompt is contributed to by several sources: the caller's text, the transparent-background
/// tag, [`QualityPreset`] tags and the [`UcPreset`] text. Anything that inspects prompt text —
/// most importantly token counting — must look at the assembled result rather than at
/// [`GenerationCore::prompt`], or it will under-report. Request building and token counting both
/// read this one type, so the counted text cannot drift from the sent text.
///
/// Obtain a value through [`GenerateImageRequest::resolve_prompts`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedPrompts {
    /// Positive prompt including the transparency and quality tags.
    pub prompt: String,
    /// Negative prompt including the UC preset text.
    pub negative_prompt: String,
    /// Character prompts in request order, empty when the model has no character support.
    ///
    /// Disabled characters are omitted; original indices are preserved.
    pub characters: Vec<ResolvedCharacterPrompt>,
}

/// Final conditioning text for one character prompt.
///
/// Character prompts receive neither quality tags nor UC preset text; they are sent as written.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedCharacterPrompt {
    /// Index in the originating [`CharacterSet::characters`].
    pub index: usize,
    /// Positive character prompt.
    pub prompt: String,
    /// Negative character prompt, empty when unset.
    pub negative_prompt: String,
}

impl Default for GenerationCore {
    fn default() -> Self {
        Self::new(String::new(), Model::default())
    }
}

/// Guidance toggles gated by model capabilities.
#[allow(clippy::struct_excessive_bools)]
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
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
pub struct OutputParams {
    /// Optional output image format.
    pub image_format: Option<ImageFormat>,
}

/// Request used by [`crate::Client::generate`].
///
/// Fields are grouped by capability: an optional group is only accepted when
/// [`Model::descriptor`] reports support for it.
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

    /// Returns the read-only descriptor of the selected model.
    #[must_use]
    pub const fn descriptor(&self) -> &'static ModelDescriptor {
        self.core.model.descriptor()
    }

    /// Returns the generation mode represented by the raw request shape.
    #[must_use]
    pub const fn mode(&self) -> GenerationMode {
        match self.img2img.as_ref() {
            None => GenerationMode::TextToImage,
            Some(input) if input.is_infill() => GenerationMode::Inpainting,
            Some(_) => GenerationMode::ImageToImage,
        }
    }

    /// Assembles already-normalized input; callers must use the generation boundary.
    pub(crate) fn assemble_prompts(&self) -> ResolvedPrompts {
        let characters = self
            .characters
            .as_ref()
            .map(|set| {
                set.characters
                    .iter()
                    .enumerate()
                    .filter(|(_, character)| character.enabled)
                    .map(|(index, character)| ResolvedCharacterPrompt {
                        index,
                        prompt: character.prompt.clone(),
                        negative_prompt: character.negative_prompt.clone().unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        ResolvedPrompts {
            prompt: self.core.assemble_prompt(),
            negative_prompt: self.core.assemble_negative_prompt(),
            characters,
        }
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

impl GenerateImageStreamRequest {
    /// Returns the underlying ordinary generation request.
    #[must_use]
    pub const fn base(&self) -> &GenerateImageRequest {
        &self.base
    }
}

impl AsRef<GenerateImageRequest> for GenerateImageStreamRequest {
    fn as_ref(&self) -> &GenerateImageRequest {
        self.base()
    }
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
