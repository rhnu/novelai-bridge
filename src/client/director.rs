use crate::{
    BridgeError, DirectorTool, EncodeVibeRequest, GeneratedImage, InvalidRequest,
    RequestCostEstimate, RunDirectorToolRequest, Transport, VibeEncoding,
    credit::VIBE_ENCODE_COST,
    estimate_director_tool_cost,
    internal::wire::{DirectorAugmentRequestBody, EncodeVibeRequestBody},
};

use super::Client;

impl<T: Transport> Client<T> {
    /// Encodes an image into a `NovelAI` vibe payload.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when local request validation, image normalization, transport, or
    /// response decoding fails.
    pub async fn encode_vibe(&self, req: EncodeVibeRequest) -> Result<VibeEncoding, BridgeError> {
        if !req.model.descriptor().can_encode_vibe() {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: "encode_vibe".to_owned(),
                required_model: "V4 and V4.5 models".to_owned(),
            }
            .into());
        }

        let information_extracted = normalize_f32_range(
            "information_extracted",
            req.information_extracted,
            0.01,
            1.0,
            req.strict_mode,
        )?;

        let payload = EncodeVibeRequestBody {
            image: req.image.to_base64(),
            information_extracted,
            model: req.model.as_str().to_owned(),
        };

        let endpoint = self.image_endpoint("/ai/encode-vibe");
        let observation = self
            .begin_simple_observation(
                crate::ObservedOperation::EncodeVibe,
                RequestCostEstimate::Fixed {
                    anlas: VIBE_ENCODE_COST,
                },
            )
            .await;
        match crate::protocol::post_bytes(&*self.transport, endpoint.as_str(), &payload).await {
            Ok(raw) => {
                let encoding = match VibeEncoding::from_bytes(raw) {
                    Ok(encoding) => encoding,
                    Err(error) => {
                        self.finish_observation_failed(observation, &error).await;
                        return Err(error);
                    }
                };
                self.finish_observation_succeeded(observation, 0).await;
                Ok(encoding)
            }
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                Err(error)
            }
        }
    }

    /// Runs a `NovelAI` director tool against an input image.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when local request validation, image decoding, transport, or
    /// response decoding fails.
    pub async fn run_director_tool(
        &self,
        req: RunDirectorToolRequest,
    ) -> Result<GeneratedImage, BridgeError> {
        let image_size = req.image.size();

        let prompt = req.prompt.as_ref().map(|value| value.trim().to_owned());
        let prompt_is_valid = prompt.as_ref().is_some_and(|value| !value.is_empty());
        let defry = req.defry.unwrap_or(0);
        if req.strict_mode && defry > 5 {
            return Err(InvalidRequest::NumericOutOfRange {
                field: "defry".to_owned(),
                value: f64::from(defry),
                min: 0.0,
                max: 5.0,
            }
            .into());
        }
        let defry = defry.min(5);

        let (resolved_prompt, resolved_defry) = match req.tool {
            DirectorTool::Lineart
            | DirectorTool::Sketch
            | DirectorTool::BgRemoval
            | DirectorTool::Declutter => {
                if prompt_is_valid || req.defry.is_some() {
                    return Err(InvalidRequest::UnsupportedFieldForContext {
                        field: "prompt or defry".to_owned(),
                        context: "lineart/sketch/bg-removal/declutter".to_owned(),
                    }
                    .into());
                }
                (None, None)
            }
            DirectorTool::Colorize => (prompt.filter(|value| !value.is_empty()), Some(defry)),
            DirectorTool::Emotion => {
                if !prompt_is_valid {
                    return Err(InvalidRequest::RequiredFieldForContext {
                        context: "emotion".to_owned(),
                        field: "prompt".to_owned(),
                    }
                    .into());
                }
                (prompt.filter(|value| !value.is_empty()), Some(defry))
            }
        };

        let payload = DirectorAugmentRequestBody {
            req_type: req.tool,
            width: image_size.width,
            height: image_size.height,
            image: req.image.to_base64(),
            prompt: resolved_prompt,
            defry: resolved_defry,
        };

        let endpoint = self.image_endpoint("/ai/augment-image");
        let estimate = estimate_director_tool_cost(image_size, self.last_pricing_context());
        let observation = self
            .begin_simple_observation(
                crate::ObservedOperation::AugmentImage,
                RequestCostEstimate::Fixed { anlas: estimate },
            )
            .await;
        match crate::protocol::post_bytes(&*self.transport, endpoint.as_str(), &payload).await {
            Ok(raw) => {
                let image = match crate::image::decode_first_generated_image(raw) {
                    Ok(image) => image,
                    Err(error) => {
                        self.finish_observation_failed(observation, &error).await;
                        return Err(error);
                    }
                };
                self.finish_observation_succeeded(observation, 1).await;
                Ok(image)
            }
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                Err(error)
            }
        }
    }
}

fn normalize_f32_range(
    field_name: &str,
    value: f32,
    min: f32,
    max: f32,
    strict_mode: bool,
) -> Result<f32, BridgeError> {
    if !value.is_finite() {
        return Err(InvalidRequest::NonFiniteNumber {
            field: field_name.to_owned(),
            value: f64::from(value),
        }
        .into());
    }
    if strict_mode && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange {
            field: field_name.to_owned(),
            value: f64::from(value),
            min: f64::from(min),
            max: f64::from(max),
        }
        .into());
    }
    Ok(value.clamp(min, max))
}
