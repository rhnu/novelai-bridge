mod build;
mod prompt;
mod references;

pub(super) use build::build_generate_request;
use prompt::{build_character_captions, build_character_prompts};
use references::convert_character_references;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct CenterPoint {
    pub(super) x: f32,
    pub(super) y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct CharacterPrompt {
    pub(super) prompt: String,
    pub(super) uc: String,
    pub(super) center: CenterPoint,
    pub(super) enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct V4CharacterCaption {
    pub(super) centers: Option<Vec<CenterPoint>>,
    pub(super) char_caption: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct V4Caption {
    pub(super) base_caption: Option<String>,
    pub(super) char_captions: Option<Vec<V4CharacterCaption>>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct V4ConditionInput {
    pub(super) caption: Option<V4Caption>,
    pub(super) legacy_uc: Option<bool>,
    pub(super) use_coords: Option<bool>,
    pub(super) use_order: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ReferenceCaption {
    pub(super) base_caption: String,
    pub(super) char_captions: Vec<V4CharacterCaption>,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ReferenceConditionInput {
    pub(super) caption: Option<ReferenceCaption>,
    pub(super) legacy_uc: Option<bool>,
}

/// `NovelAI` image parameters.
///
/// Every field is optional so that capability-driven assembly can omit whatever a model family
/// does not accept. `skip_serializing_none` keeps omitted fields out of the payload entirely
/// rather than sending explicit nulls.
#[skip_serializing_none]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ImageParameters {
    #[serde(rename = "params_version")]
    pub(super) params_version: Option<u32>,
    pub(super) width: Option<u32>,
    pub(super) height: Option<u32>,
    pub(super) steps: Option<u32>,
    pub(super) scale: Option<f32>,
    pub(super) sampler: Option<String>,
    pub(super) seed: Option<i64>,
    #[serde(rename = "n_samples")]
    pub(super) n_samples: Option<u32>,
    pub(super) prompt: Option<String>,
    pub(super) negative_prompt: Option<String>,
    /// Legacy top-level negative prompt used by V3 and earlier.
    pub(super) uc: Option<String>,
    #[serde(rename = "ucPreset")]
    pub(super) uc_preset: Option<i32>,
    #[serde(rename = "qualityToggle")]
    pub(super) quality_toggle: Option<bool>,
    #[serde(rename = "v4_prompt")]
    pub(super) v4_prompt: Option<V4ConditionInput>,
    #[serde(rename = "v4_negative_prompt")]
    pub(super) v4_negative_prompt: Option<V4ConditionInput>,
    #[serde(rename = "cfg_rescale")]
    pub(super) cfg_rescale: Option<f32>,
    #[serde(rename = "noise_schedule")]
    pub(super) noise_schedule: Option<String>,
    #[serde(rename = "skip_cfg_above_sigma")]
    pub(super) skip_cfg_above_sigma: Option<f32>,
    /// Set to `NativeInfillingRequest` for infill requests.
    #[serde(rename = "request_type")]
    pub(super) request_type: Option<String>,
    pub(super) image: Option<String>,
    pub(super) strength: Option<f32>,
    pub(super) noise: Option<f32>,
    #[serde(rename = "extra_noise_seed")]
    pub(super) extra_noise_seed: Option<i64>,
    #[serde(rename = "color_correct")]
    pub(super) color_correct: Option<bool>,
    pub(super) mask: Option<String>,
    #[serde(rename = "reference_image_multiple")]
    pub(super) reference_image_multiple: Option<Vec<String>>,
    #[serde(rename = "reference_strength_multiple")]
    pub(super) reference_strength_multiple: Option<Vec<f32>>,
    #[serde(rename = "normalize_reference_strength_multiple")]
    pub(super) normalize_reference_strength_multiple: Option<bool>,
    #[serde(rename = "controlnet_strength")]
    pub(super) controlnet_strength: Option<f32>,
    #[serde(rename = "director_reference_images")]
    pub(super) director_reference_images: Option<Vec<String>>,
    #[serde(rename = "director_reference_descriptions")]
    pub(super) director_reference_descriptions: Option<Vec<ReferenceConditionInput>>,
    #[serde(rename = "director_reference_information_extracted")]
    pub(super) director_reference_information_extracted: Option<Vec<f32>>,
    #[serde(rename = "director_reference_secondary_strength_values")]
    pub(super) director_reference_secondary_strength_values: Option<Vec<f32>>,
    #[serde(rename = "director_reference_strength_values")]
    pub(super) director_reference_strength_values: Option<Vec<f32>>,
    #[serde(rename = "characterPrompts")]
    pub(super) character_prompts: Option<Vec<CharacterPrompt>>,
    pub(super) legacy: Option<bool>,
    #[serde(rename = "legacy_uc")]
    pub(super) legacy_uc: Option<bool>,
    #[serde(rename = "legacy_v3_extend")]
    pub(super) legacy_v3_extend: Option<bool>,
    #[serde(rename = "deliberate_euler_ancestral_bug")]
    pub(super) deliberate_euler_ancestral_bug: Option<bool>,
    #[serde(rename = "prefer_brownian")]
    pub(super) prefer_brownian: Option<bool>,
    #[serde(rename = "autoSmea")]
    pub(super) auto_smea: Option<bool>,
    pub(super) sm: Option<bool>,
    #[serde(rename = "sm_dyn")]
    pub(super) sm_dyn: Option<bool>,
    #[serde(rename = "dynamic_thresholding")]
    pub(super) dynamic_thresholding: Option<bool>,
    #[serde(rename = "add_original_image")]
    pub(super) add_original_image: Option<bool>,
    #[serde(rename = "inpaintImg2ImgStrength")]
    pub(super) inpaint_img2_img_strength: Option<f32>,
    #[serde(rename = "tag_hint_transparent_background")]
    pub(super) tag_hint_transparent_background: Option<bool>,
    #[serde(rename = "use_coords")]
    pub(super) use_coords: Option<bool>,
    #[serde(rename = "image_format")]
    pub(super) image_format: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ImageStreamParameters<'payload> {
    #[serde(flatten)]
    pub(super) base: &'payload ImageParameters,
    pub(super) stream: &'static str,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct ImageGenerationRequestBody {
    pub(super) action: String,
    pub(super) input: String,
    pub(super) model: String,
    #[serde(rename = "use_new_shared_trial")]
    pub(super) use_new_shared_trial: bool,
    pub(super) parameters: ImageParameters,
}

#[derive(Debug, Serialize)]
pub(super) struct StreamImageGenerationRequestBody<'payload> {
    pub(super) action: &'payload str,
    pub(super) input: &'payload str,
    pub(super) model: &'payload str,
    #[serde(rename = "use_new_shared_trial")]
    pub(super) use_new_shared_trial: bool,
    pub(super) parameters: ImageStreamParameters<'payload>,
}
