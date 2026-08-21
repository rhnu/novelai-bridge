use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

use crate::DirectorTool;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CenterPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterPrompt {
    pub prompt: String,
    pub uc: String,
    pub center: CenterPoint,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct V4CharacterCaption {
    pub centers: Option<Vec<CenterPoint>>,
    pub char_caption: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct V4Caption {
    pub base_caption: Option<String>,
    pub char_captions: Option<Vec<V4CharacterCaption>>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct V4ConditionInput {
    pub caption: Option<V4Caption>,
    pub legacy_uc: Option<bool>,
    pub use_coords: Option<bool>,
    pub use_order: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReferenceCaption {
    pub base_caption: String,
    pub char_captions: Vec<V4CharacterCaption>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReferenceConditionInput {
    pub caption: Option<ReferenceCaption>,
    pub legacy_uc: Option<bool>,
}

/// `NovelAI` image parameters.
///
/// Every field is optional so that capability-driven assembly can omit whatever a model family
/// does not accept. `skip_serializing_none` keeps omitted fields out of the payload entirely
/// rather than sending explicit nulls.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageParameters {
    #[serde(rename = "params_version")]
    pub params_version: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub scale: Option<f32>,
    pub sampler: Option<String>,
    pub seed: Option<i64>,
    #[serde(rename = "n_samples")]
    pub n_samples: Option<u32>,
    pub prompt: Option<String>,
    pub negative_prompt: Option<String>,
    /// Legacy top-level negative prompt used by V3 and earlier.
    pub uc: Option<String>,
    #[serde(rename = "ucPreset")]
    pub uc_preset: Option<i32>,
    #[serde(rename = "qualityToggle")]
    pub quality_toggle: Option<bool>,
    #[serde(rename = "v4_prompt")]
    pub v4_prompt: Option<V4ConditionInput>,
    #[serde(rename = "v4_negative_prompt")]
    pub v4_negative_prompt: Option<V4ConditionInput>,
    #[serde(rename = "cfg_rescale")]
    pub cfg_rescale: Option<f32>,
    #[serde(rename = "noise_schedule")]
    pub noise_schedule: Option<String>,
    #[serde(rename = "skip_cfg_above_sigma")]
    pub skip_cfg_above_sigma: Option<f32>,
    /// Set to `NativeInfillingRequest` for infill requests.
    #[serde(rename = "request_type")]
    pub request_type: Option<String>,
    pub image: Option<String>,
    pub strength: Option<f32>,
    pub noise: Option<f32>,
    #[serde(rename = "extra_noise_seed")]
    pub extra_noise_seed: Option<i64>,
    #[serde(rename = "color_correct")]
    pub color_correct: Option<bool>,
    pub mask: Option<String>,
    #[serde(rename = "reference_image_multiple")]
    pub reference_image_multiple: Option<Vec<String>>,
    #[serde(rename = "reference_strength_multiple")]
    pub reference_strength_multiple: Option<Vec<f32>>,
    #[serde(rename = "normalize_reference_strength_multiple")]
    pub normalize_reference_strength_multiple: Option<bool>,
    #[serde(rename = "controlnet_strength")]
    pub controlnet_strength: Option<f32>,
    #[serde(rename = "director_reference_images")]
    pub director_reference_images: Option<Vec<String>>,
    #[serde(rename = "director_reference_descriptions")]
    pub director_reference_descriptions: Option<Vec<ReferenceConditionInput>>,
    #[serde(rename = "director_reference_information_extracted")]
    pub director_reference_information_extracted: Option<Vec<f32>>,
    #[serde(rename = "director_reference_secondary_strength_values")]
    pub director_reference_secondary_strength_values: Option<Vec<f32>>,
    #[serde(rename = "director_reference_strength_values")]
    pub director_reference_strength_values: Option<Vec<f32>>,
    #[serde(rename = "characterPrompts")]
    pub character_prompts: Option<Vec<CharacterPrompt>>,
    pub legacy: Option<bool>,
    #[serde(rename = "legacy_uc")]
    pub legacy_uc: Option<bool>,
    #[serde(rename = "legacy_v3_extend")]
    pub legacy_v3_extend: Option<bool>,
    #[serde(rename = "deliberate_euler_ancestral_bug")]
    pub deliberate_euler_ancestral_bug: Option<bool>,
    #[serde(rename = "prefer_brownian")]
    pub prefer_brownian: Option<bool>,
    #[serde(rename = "autoSmea")]
    pub auto_smea: Option<bool>,
    pub sm: Option<bool>,
    #[serde(rename = "sm_dyn")]
    pub sm_dyn: Option<bool>,
    #[serde(rename = "dynamic_thresholding")]
    pub dynamic_thresholding: Option<bool>,
    #[serde(rename = "add_original_image")]
    pub add_original_image: Option<bool>,
    #[serde(rename = "inpaintImg2ImgStrength")]
    pub inpaint_img2_img_strength: Option<f32>,
    #[serde(rename = "tag_hint_transparent_background")]
    pub tag_hint_transparent_background: Option<bool>,
    #[serde(rename = "use_coords")]
    pub use_coords: Option<bool>,
    #[serde(rename = "image_format")]
    pub image_format: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageStreamParameters {
    #[serde(flatten)]
    pub base: ImageParameters,
    pub stream: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ImageGenerationRequestBody {
    pub action: String,
    pub input: String,
    pub model: String,
    #[serde(rename = "use_new_shared_trial")]
    pub use_new_shared_trial: bool,
    pub parameters: ImageParameters,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StreamImageGenerationRequestBody {
    pub action: String,
    pub input: String,
    pub model: String,
    #[serde(rename = "use_new_shared_trial")]
    pub use_new_shared_trial: bool,
    pub parameters: ImageStreamParameters,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EncodeVibeRequestBody {
    pub image: String,
    #[serde(rename = "information_extracted")]
    pub information_extracted: f32,
    pub model: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DirectorAugmentRequestBody {
    pub req_type: DirectorTool,
    pub width: u32,
    pub height: u32,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defry: Option<u8>,
}
