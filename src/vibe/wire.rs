use serde::Serialize;
use serde_with::skip_serializing_none;

#[skip_serializing_none]
#[derive(Serialize)]
pub(super) struct VibeRequestBody {
    pub image: String,
    pub information_extracted: f32,
    pub model: &'static str,
    pub mask: Option<String>,
    pub crop_to_mask: Option<bool>,
    pub focus_seed: Option<i64>,
    pub info_extract_seed: Option<i64>,
}
