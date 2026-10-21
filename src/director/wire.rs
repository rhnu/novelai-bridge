use serde::Serialize;

#[derive(Serialize)]
pub(super) struct DirectorRequestBody {
    pub req_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub image: crate::image::request_body::Media,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defry: Option<u8>,
}
