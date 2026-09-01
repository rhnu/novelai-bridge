use crate::{
    BridgeError, DirectorCommand, EncodeVibeRequest, GeneratedImage, InvalidRequest,
    RequestCostEstimate, RunDirectorToolRequest, Transport, VibeEncoding,
    estimate_director_tool_cost, pricing::VIBE_ENCODE_COST,
};

mod wire;

use wire::{DirectorAugmentRequestBody, EncodeVibeRequestBody};

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

        let (req_type, resolved_prompt, resolved_defry) =
            resolve_director_command(req.command, req.strict_mode)?;

        let payload = DirectorAugmentRequestBody {
            req_type,
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

fn resolve_director_command(
    command: DirectorCommand,
    strict_mode: bool,
) -> Result<(&'static str, Option<String>, Option<u8>), BridgeError> {
    let (req_type, prompt, defry) = match command {
        DirectorCommand::Lineart => ("lineart", None, None),
        DirectorCommand::Sketch => ("sketch", None, None),
        DirectorCommand::BgRemoval => ("bg-removal", None, None),
        DirectorCommand::Declutter => ("declutter", None, None),
        DirectorCommand::Colorize { prompt, defry } => (
            "colorize",
            prompt
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            Some(normalize_defry(defry, strict_mode)?),
        ),
        DirectorCommand::Emotion { prompt, defry } => {
            let prompt = prompt.trim().to_owned();
            if prompt.is_empty() {
                return Err(InvalidRequest::RequiredFieldForContext {
                    context: "emotion".to_owned(),
                    field: "prompt".to_owned(),
                }
                .into());
            }
            (
                "emotion",
                Some(prompt),
                Some(normalize_defry(defry, strict_mode)?),
            )
        }
    };
    Ok((req_type, prompt, defry))
}

fn normalize_defry(defry: u8, strict_mode: bool) -> Result<u8, BridgeError> {
    if strict_mode && defry > 5 {
        return Err(InvalidRequest::NumericOutOfRange {
            field: "defry".to_owned(),
            value: f64::from(defry),
            min: 0.0,
            max: 5.0,
        }
        .into());
    }
    Ok(defry.min(5))
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
