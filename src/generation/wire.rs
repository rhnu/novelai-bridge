//! Serialization-only components for validated generation plans.
mod build;
mod prompt;
mod references;

pub(super) use build::build_generate_request;
use references::ReferenceParameters;
use serde::Serialize;
use serde_with::skip_serializing_none;

use crate::{ImageFormat, NoiseSchedule, Sampler};

#[derive(Serialize)]
pub(super) struct CenterPoint {
    x: f32,
    y: f32,
}

#[derive(Serialize)]
pub(super) struct CharacterPrompt {
    prompt: String,
    uc: String,
    center: CenterPoint,
    enabled: bool,
}

#[derive(Serialize)]
pub(super) struct V4CharacterCaption {
    centers: Vec<CenterPoint>,
    char_caption: String,
}

#[derive(Serialize)]
pub(super) struct V4Caption {
    base_caption: String,
    char_captions: Vec<V4CharacterCaption>,
}

#[derive(Serialize)]
pub(super) struct PositiveCondition {
    caption: V4Caption,
    use_coords: bool,
    use_order: bool,
}

#[derive(Serialize)]
pub(super) struct NegativeCondition {
    caption: V4Caption,
}

#[derive(Serialize)]
pub(super) struct ReferenceCaption {
    base_caption: &'static str,
    char_captions: Vec<V4CharacterCaption>,
}

#[derive(Serialize)]
pub(super) struct ReferenceConditionInput {
    caption: ReferenceCaption,
    legacy_uc: bool,
}

/// Fields required for every generation request, with typed API identifiers.
#[skip_serializing_none]
#[derive(Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent provider protocol flags have fixed JSON boolean representations"
)]
pub(super) struct CommonParameters {
    params_version: u32,
    width: u32,
    height: u32,
    steps: u32,
    scale: f32,
    sampler: Sampler,
    seed: i64,
    n_samples: u32,
    #[serde(rename = "ucPreset")]
    uc_preset: i32,
    #[serde(rename = "qualityToggle")]
    quality_toggle: bool,
    cfg_rescale: f32,
    noise_schedule: NoiseSchedule,
    legacy: bool,
    legacy_v3_extend: bool,
    deliberate_euler_ancestral_bug: bool,
    prefer_brownian: bool,
    #[serde(rename = "autoSmea")]
    auto_smea: bool,
    image_format: Option<ImageFormat>,
}

/// Flat and structured conditioning are exclusive wire representations.
#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ConditioningParameters {
    Legacy {
        prompt: String,
        negative_prompt: String,
        uc: String,
    },
    Structured {
        negative_prompt: String,
        v4_prompt: PositiveCondition,
        v4_negative_prompt: NegativeCondition,
        #[serde(rename = "characterPrompts")]
        character_prompts: Vec<CharacterPrompt>,
        use_coords: bool,
    },
}

/// Only the selected operation's valid fields can be emitted.
#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum OperationParameters {
    TextToImage {
        add_original_image: bool,
        #[serde(rename = "inpaintImg2ImgStrength")]
        inpaint_strength: f32,
    },
    ImageToImage {
        image: String,
        strength: f32,
        noise: f32,
        extra_noise_seed: i64,
        color_correct: bool,
        add_original_image: bool,
        #[serde(rename = "inpaintImg2ImgStrength")]
        inpaint_strength: f32,
    },
    Inpainting {
        image: String,
        mask: String,
        noise: f32,
        request_type: &'static str,
        add_original_image: bool,
        #[serde(rename = "inpaintImg2ImgStrength")]
        strength: f32,
    },
}

/// Optional sampling and prompt hints are independent capabilities.
#[skip_serializing_none]
#[derive(Serialize)]
pub(super) struct CapabilityParameters {
    skip_cfg_above_sigma: Option<f32>,
    sm: Option<bool>,
    sm_dyn: Option<bool>,
    dynamic_thresholding: Option<bool>,
    tag_hint_transparent_background: Option<bool>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ProtocolExtensions {
    Standard {},
    V5 { legacy_uc: bool },
}

#[derive(Serialize)]
pub(super) struct ImageParameters {
    #[serde(flatten)]
    common: CommonParameters,
    #[serde(flatten)]
    conditioning: ConditioningParameters,
    #[serde(flatten)]
    operation: OperationParameters,
    #[serde(flatten)]
    capabilities: CapabilityParameters,
    #[serde(flatten)]
    references: ReferenceParameters,
    #[serde(flatten)]
    extensions: ProtocolExtensions,
}

#[derive(Serialize)]
pub(super) struct ImageStreamParameters<'payload> {
    #[serde(flatten)]
    pub(super) base: &'payload ImageParameters,
    pub(super) stream: &'static str,
}

#[derive(Copy, Clone, Serialize)]
pub(super) enum WireAction {
    #[serde(rename = "generate")]
    Generate,
    #[serde(rename = "img2img")]
    ImageToImage,
    #[serde(rename = "infill")]
    Inpainting,
}

#[derive(Serialize)]
pub(super) struct ImageGenerationRequestBody {
    pub(super) action: WireAction,
    pub(super) input: String,
    pub(super) model: &'static str,
    pub(super) use_new_shared_trial: bool,
    pub(super) parameters: ImageParameters,
}

#[derive(Serialize)]
pub(super) struct StreamImageGenerationRequestBody<'payload> {
    pub(super) action: WireAction,
    pub(super) input: &'payload str,
    pub(super) model: &'static str,
    pub(super) use_new_shared_trial: bool,
    pub(super) parameters: ImageStreamParameters<'payload>,
}
