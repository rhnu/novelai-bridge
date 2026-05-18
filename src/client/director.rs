use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    BridgeError, DirectorTool, EncodeVibeRequest, GeneratedImage, RunDirectorToolRequest,
    Transport,
    internal::{
        request_helpers::{extract_first_image, normalize_image_payload},
        wire::{DirectorAugmentRequestBody, EncodeVibeRequestBody},
    },
};

use super::Client;

impl<T: Transport> Client<T> {
    /// Encodes an image into a NovelAI vibe payload.
    ///
    /// `req.image` accepts either a raw base64 payload or an image data URL.
    pub async fn encode_vibe(&self, req: EncodeVibeRequest) -> Result<String, BridgeError> {
        let information_extracted = normalize_f32_range(
            "information_extracted",
            req.information_extracted,
            0.01,
            1.0,
            req.strict_mode,
            "information_extracted must be between 0.01 and 1.0.",
        )?;

        let normalized_image = normalize_image_payload("encode_vibe.image", &req.image)?;
        let payload = EncodeVibeRequestBody {
            image: normalized_image,
            information_extracted,
            model: req.model.as_str().to_owned(),
        };

        let endpoint = self.image_endpoint("/ai/encode-vibe");
        let raw = self
            .transport
            .post_bytes(endpoint.as_str(), &payload)
            .await?;
        Ok(STANDARD.encode(raw))
    }

    /// Runs a NovelAI director tool against an input image.
    ///
    /// `req.image` accepts either a raw base64 payload or an image data URL.
    pub async fn run_director_tool(
        &self,
        req: RunDirectorToolRequest,
    ) -> Result<GeneratedImage, BridgeError> {
        let normalized_image = normalize_image_payload("director.image", &req.image)?;
        let decoded_image = STANDARD.decode(normalized_image.as_bytes()).map_err(|_| {
            BridgeError::invalid_request("director.image must be valid base64 image payload.")
        })?;
        let decoded = image::load_from_memory(&decoded_image).map_err(|_| {
            BridgeError::invalid_request("director.image must be a decodable image payload.")
        })?;

        let prompt = req.prompt.as_ref().map(|value| value.trim().to_owned());
        let prompt_is_valid = prompt.as_ref().is_some_and(|value| !value.is_empty());
        let defry = req.defry.unwrap_or(0);
        if req.strict_mode && defry > 5 {
            return Err(BridgeError::invalid_request(
                "defry must be in range 0..=5.",
            ));
        }
        let defry = defry.min(5);

        let (resolved_prompt, resolved_defry) = match req.tool {
            DirectorTool::Lineart
            | DirectorTool::Sketch
            | DirectorTool::BgRemoval
            | DirectorTool::Declutter => {
                if prompt_is_valid || req.defry.is_some() {
                    return Err(BridgeError::invalid_request(
                        "lineart/sketch/bg-removal/declutter do not accept prompt or defry.",
                    ));
                }
                (None, None)
            }
            DirectorTool::Colorize => (prompt.filter(|value| !value.is_empty()), Some(defry)),
            DirectorTool::Emotion => {
                if !prompt_is_valid {
                    return Err(BridgeError::invalid_request(
                        "emotion requires non-empty prompt.",
                    ));
                }
                (prompt.filter(|value| !value.is_empty()), Some(defry))
            }
        };

        let payload = DirectorAugmentRequestBody {
            req_type: req.tool,
            width: decoded.width(),
            height: decoded.height(),
            image: normalized_image,
            prompt: resolved_prompt,
            defry: resolved_defry,
        };

        let endpoint = self.image_endpoint("/ai/augment-image");
        let raw = self
            .transport
            .post_bytes(endpoint.as_str(), &payload)
            .await?;
        Ok(extract_first_image(raw))
    }

    pub(super) fn resolve_vibe_data_cache(&self, value: &str) -> Result<String, BridgeError> {
        let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
        if compact.is_empty() {
            return Err(BridgeError::invalid_request(
                "vibe_data_cache must be base64 payload.",
            ));
        }

        STANDARD
            .decode(compact.as_bytes())
            .map_err(|_| BridgeError::invalid_request("vibe_data_cache must be base64 payload."))?;

        Ok(compact)
    }
}

fn normalize_f32_range(
    field_name: &str,
    value: f32,
    min: f32,
    max: f32,
    strict_mode: bool,
    range_message: &'static str,
) -> Result<f32, BridgeError> {
    if !value.is_finite() {
        return Err(BridgeError::invalid_request(format!(
            "{field_name} must be finite."
        )));
    }
    if strict_mode && !(min..=max).contains(&value) {
        return Err(BridgeError::invalid_request(range_message));
    }
    Ok(value.clamp(min, max))
}
