use serde::Serialize;

#[derive(Debug, Serialize)]
pub(super) struct VibeRequestBody {
    pub image: String,
    pub information_extracted: f32,
    pub model: &'static str,
}
