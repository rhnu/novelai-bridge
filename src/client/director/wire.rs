use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct EncodeVibeRequestBody {
    pub image: String,
    #[serde(rename = "information_extracted")]
    pub information_extracted: f32,
    pub model: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DirectorAugmentRequestBody {
    pub req_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defry: Option<u8>,
}
