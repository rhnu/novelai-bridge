mod build;
mod prompt;
mod references;

pub(super) use build::build_generate_request;
pub(super) use prompt::{build_character_captions, build_character_prompts};
pub(super) use references::convert_character_references;

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct CenterPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct CharacterPrompt {
    pub(crate) prompt: String,
    pub(crate) uc: String,
    pub(crate) center: CenterPoint,
    pub(crate) enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct V4CharacterCaption {
    pub(crate) centers: Option<Vec<CenterPoint>>,
    pub(crate) char_caption: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct V4Caption {
    pub(crate) base_caption: Option<String>,
    pub(crate) char_captions: Option<Vec<V4CharacterCaption>>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct V4ConditionInput {
    pub(crate) caption: Option<V4Caption>,
    pub(crate) legacy_uc: Option<bool>,
    pub(crate) use_coords: Option<bool>,
    pub(crate) use_order: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ReferenceCaption {
    pub(crate) base_caption: String,
    pub(crate) char_captions: Vec<V4CharacterCaption>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ReferenceConditionInput {
    pub(crate) caption: Option<ReferenceCaption>,
    pub(crate) legacy_uc: Option<bool>,
}

/// `NovelAI` image parameters.
///
/// Every field is optional so that capability-driven assembly can omit whatever a model family
/// does not accept. `skip_serializing_none` keeps omitted fields out of the payload entirely
/// rather than sending explicit nulls.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ImageParameters {
    #[serde(rename = "params_version")]
    pub(crate) params_version: Option<u32>,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) steps: Option<u32>,
    pub(crate) scale: Option<f32>,
    pub(crate) sampler: Option<String>,
    pub(crate) seed: Option<i64>,
    #[serde(rename = "n_samples")]
    pub(crate) n_samples: Option<u32>,
    pub(crate) prompt: Option<String>,
    pub(crate) negative_prompt: Option<String>,
    /// Legacy top-level negative prompt used by V3 and earlier.
    pub(crate) uc: Option<String>,
    #[serde(rename = "ucPreset")]
    pub(crate) uc_preset: Option<i32>,
    #[serde(rename = "qualityToggle")]
    pub(crate) quality_toggle: Option<bool>,
    #[serde(rename = "v4_prompt")]
    pub(crate) v4_prompt: Option<V4ConditionInput>,
    #[serde(rename = "v4_negative_prompt")]
    pub(crate) v4_negative_prompt: Option<V4ConditionInput>,
    #[serde(rename = "cfg_rescale")]
    pub(crate) cfg_rescale: Option<f32>,
    #[serde(rename = "noise_schedule")]
    pub(crate) noise_schedule: Option<String>,
    #[serde(rename = "skip_cfg_above_sigma")]
    pub(crate) skip_cfg_above_sigma: Option<f32>,
    /// Set to `NativeInfillingRequest` for infill requests.
    #[serde(rename = "request_type")]
    pub(crate) request_type: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) strength: Option<f32>,
    pub(crate) noise: Option<f32>,
    #[serde(rename = "extra_noise_seed")]
    pub(crate) extra_noise_seed: Option<i64>,
    #[serde(rename = "color_correct")]
    pub(crate) color_correct: Option<bool>,
    pub(crate) mask: Option<String>,
    #[serde(rename = "reference_image_multiple")]
    pub(crate) reference_image_multiple: Option<Vec<String>>,
    #[serde(rename = "reference_strength_multiple")]
    pub(crate) reference_strength_multiple: Option<Vec<f32>>,
    #[serde(rename = "normalize_reference_strength_multiple")]
    pub(crate) normalize_reference_strength_multiple: Option<bool>,
    #[serde(rename = "controlnet_strength")]
    pub(crate) controlnet_strength: Option<f32>,
    #[serde(rename = "director_reference_images")]
    pub(crate) director_reference_images: Option<Vec<String>>,
    #[serde(rename = "director_reference_descriptions")]
    pub(crate) director_reference_descriptions: Option<Vec<ReferenceConditionInput>>,
    #[serde(rename = "director_reference_information_extracted")]
    pub(crate) director_reference_information_extracted: Option<Vec<f32>>,
    #[serde(rename = "director_reference_secondary_strength_values")]
    pub(crate) director_reference_secondary_strength_values: Option<Vec<f32>>,
    #[serde(rename = "director_reference_strength_values")]
    pub(crate) director_reference_strength_values: Option<Vec<f32>>,
    #[serde(rename = "characterPrompts")]
    pub(crate) character_prompts: Option<Vec<CharacterPrompt>>,
    pub(crate) legacy: Option<bool>,
    #[serde(rename = "legacy_uc")]
    pub(crate) legacy_uc: Option<bool>,
    #[serde(rename = "legacy_v3_extend")]
    pub(crate) legacy_v3_extend: Option<bool>,
    #[serde(rename = "deliberate_euler_ancestral_bug")]
    pub(crate) deliberate_euler_ancestral_bug: Option<bool>,
    #[serde(rename = "prefer_brownian")]
    pub(crate) prefer_brownian: Option<bool>,
    #[serde(rename = "autoSmea")]
    pub(crate) auto_smea: Option<bool>,
    pub(crate) sm: Option<bool>,
    #[serde(rename = "sm_dyn")]
    pub(crate) sm_dyn: Option<bool>,
    #[serde(rename = "dynamic_thresholding")]
    pub(crate) dynamic_thresholding: Option<bool>,
    #[serde(rename = "add_original_image")]
    pub(crate) add_original_image: Option<bool>,
    #[serde(rename = "inpaintImg2ImgStrength")]
    pub(crate) inpaint_img2_img_strength: Option<f32>,
    #[serde(rename = "tag_hint_transparent_background")]
    pub(crate) tag_hint_transparent_background: Option<bool>,
    #[serde(rename = "use_coords")]
    pub(crate) use_coords: Option<bool>,
    #[serde(rename = "image_format")]
    pub(crate) image_format: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ImageStreamParameters<'payload> {
    #[serde(flatten)]
    pub(crate) base: &'payload ImageParameters,
    pub(crate) stream: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct ImageGenerationRequestBody {
    pub(crate) action: String,
    pub(crate) input: String,
    pub(crate) model: String,
    #[serde(rename = "use_new_shared_trial")]
    pub(crate) use_new_shared_trial: bool,
    pub(crate) parameters: ImageParameters,
}

#[derive(Debug, Serialize)]
pub(crate) struct StreamImageGenerationRequestBody<'payload> {
    pub(crate) action: &'payload str,
    pub(crate) input: &'payload str,
    pub(crate) model: &'payload str,
    #[serde(rename = "use_new_shared_trial")]
    pub(crate) use_new_shared_trial: bool,
    pub(crate) parameters: ImageStreamParameters<'payload>,
}
