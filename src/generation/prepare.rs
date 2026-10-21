use super::{
    GenerationGeometry, NormalizedGeneration,
    wire::{
        ImageGenerationRequestBody, ImageStreamParameters, StreamImageGenerationRequestBody,
        build_generate_request,
    },
};
use crate::{
    BridgeError, EncodeError, EncodeTarget, ImagePreprocessor, InvalidRequest, RequestField,
    StreamMode,
};

/// Wire delivery selected after seed and image preparation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GenerationDelivery {
    /// Ordinary image response.
    Images,
    /// Incremental streamed response.
    Stream(StreamMode),
}

/// Image-prepared input for one sending attempt, with a fixed resolved seed.
///
/// This type cannot be deserialized or mutated into an unvalidated request. It does not send
/// network traffic or retry. Image preparation may allocate and decode image pixels.
pub struct PreparedGeneration {
    normalized: NormalizedGeneration,
    body: ImageGenerationRequestBody,
    resolved_seed: i64,
}

impl std::fmt::Debug for PreparedGeneration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let request = self.normalized.request();
        formatter
            .debug_struct("PreparedGeneration")
            .field("model", &request.core.model)
            .field("mode", &request.mode())
            .field("size", &request.core.size)
            .field("geometry", self.normalized.geometry())
            .field("samples", &request.core.n_samples)
            .field("resolved_seed", &self.resolved_seed)
            .field("diagnostics", &self.normalized.diagnostics().len())
            .finish_non_exhaustive()
    }
}

impl PreparedGeneration {
    pub(crate) fn take_request_body(
        &mut self,
        delivery: GenerationDelivery,
    ) -> Result<crate::image::request_body::RequestBody, BridgeError> {
        if matches!(delivery, GenerationDelivery::Stream(_)) {
            self.validate_stream()?;
        }
        let media = self.body.extract_media();
        let mut body = crate::image::request_body::MediaBody::new(&self.body, media)?;
        if let GenerationDelivery::Stream(mode) = delivery {
            body.value["parameters"]["stream"] =
                serde_json::Value::String(mode.as_str().to_owned());
        }
        Ok(crate::image::request_body::RequestBody::Media(body))
    }

    fn validate_stream(&self) -> Result<(), BridgeError> {
        if !self.normalized.request().descriptor().support().streaming {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: RequestField::StreamDelivery,
                required_model: "a model supporting streaming".to_owned(),
            }
            .into());
        }
        Ok(())
    }

    /// Effective input and cached prompts used to construct this payload.
    #[must_use]
    pub const fn normalized(&self) -> &NormalizedGeneration { &self.normalized }

    /// Request, expected-output and quote dimensions fixed by normalization.
    #[must_use]
    pub const fn geometry(&self) -> &GenerationGeometry { self.normalized.geometry() }

    /// Request-level seed selected once during preparation.
    #[must_use]
    pub const fn resolved_seed(&self) -> i64 { self.resolved_seed }

    /// Encodes this fixed preparation for the selected delivery, without preparing images again.
    ///
    /// # Errors
    ///
    /// Returns unsupported-delivery or JSON encoding failures.
    pub fn encode(&self, delivery: GenerationDelivery) -> Result<Vec<u8>, BridgeError> {
        let encode_error = |source| EncodeError::new(EncodeTarget::JsonRequest, Box::new(source));
        match delivery {
            GenerationDelivery::Images => serde_json::to_vec(&self.body)
                .map_err(encode_error)
                .map_err(Into::into),
            GenerationDelivery::Stream(mode) => {
                self.validate_stream()?;
                let body = StreamImageGenerationRequestBody {
                    action: self.body.action,
                    input: &self.body.input,
                    model: self.body.model,
                    use_new_shared_trial: self.body.use_new_shared_trial,
                    parameters: ImageStreamParameters {
                        base: &self.body.parameters,
                        stream: mode.as_str(),
                    },
                };
                serde_json::to_vec(&body)
                    .map_err(encode_error)
                    .map_err(Into::into)
            },
        }
    }
}

pub(super) fn prepare(normalized: NormalizedGeneration) -> Result<PreparedGeneration, BridgeError> {
    prepare_inner(normalized, None)
}

pub(super) fn prepare_with(
    normalized: NormalizedGeneration,
    preprocessor: &dyn ImagePreprocessor,
) -> Result<PreparedGeneration, BridgeError> {
    prepare_inner(normalized, Some(preprocessor))
}

fn prepare_inner(
    normalized: NormalizedGeneration,
    preprocessor: Option<&dyn ImagePreprocessor>,
) -> Result<PreparedGeneration, BridgeError> {
    let resolved_seed = super::seed::resolve_seed(normalized.request().core.seed)?;
    let body = build_generate_request(&normalized, resolved_seed, preprocessor)?;
    Ok(PreparedGeneration {
        normalized,
        body,
        resolved_seed,
    })
}

#[cfg(test)]
mod tests;
