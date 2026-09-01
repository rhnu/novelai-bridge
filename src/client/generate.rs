use super::Client;
use crate::{
    BridgeError, GenerateImageRequest, GenerateImageResult, GenerateImageStream,
    GenerateImageStreamRequest, InvalidRequest, PreparedGeneration, StreamMode, StreamingTransport,
    Transport, decode_generated_images,
    internal::wire::{ImageStreamParameters, StreamImageGenerationRequestBody},
};

impl<T: Transport> Client<T> {
    /// Generates images from a request and returns the decoded image payloads.
    ///
    /// The client validates request ranges, model capabilities and payload shape locally, and
    /// fits image inputs to the requested canvas, but it intentionally does not parse or reject
    /// prompt syntax before sending the request.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when request validation, image normalization, transport, or response
    /// decoding fails.
    pub async fn generate(
        &self,
        params: GenerateImageRequest,
    ) -> Result<GenerateImageResult, BridgeError> {
        let params = params.normalize()?;
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(&params)?;
        self.send_prepared(params.prepare()?).await
    }

    /// Sends an image-prepared request without repeating normalization or resolving its seed.
    ///
    /// # Errors
    ///
    /// Returns token-limit, transport or response errors.
    pub async fn generate_prepared(
        &self,
        prepared: PreparedGeneration,
    ) -> Result<GenerateImageResult, BridgeError> {
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(prepared.normalized())?;
        self.send_prepared(prepared).await
    }

    async fn send_prepared(
        &self,
        prepared: PreparedGeneration,
    ) -> Result<GenerateImageResult, BridgeError> {
        let (params, request, resolved_seed) = prepared.into_parts();
        let endpoint = self.image_endpoint("/ai/generate-image");

        let observation = self
            .begin_generation_observation(&params, crate::ObservedOperation::GenerateImage)
            .await;
        let payload = match crate::protocol::post_bytes(
            &*self.transport,
            endpoint.as_str(),
            &request,
        )
        .await
        {
            Ok(payload) => payload,
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                return Err(error);
            }
        };

        let images = match decode_generated_images(payload) {
            Ok(images) => images,
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                return Err(error);
            }
        };
        let image_count = u32::try_from(images.len()).unwrap_or(u32::MAX);
        self.finish_observation_succeeded(observation, image_count)
            .await;

        Ok(GenerateImageResult {
            resolved_seed,
            images,
        })
    }

    /// Starts a streaming image generation request and returns parsed SSE chunks.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when request validation, image normalization, transport, or stream
    /// setup fails.
    pub async fn generate_stream(
        &self,
        params: GenerateImageStreamRequest,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        if !params.base.core.model.descriptor().support().streaming {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: "generate_stream".to_owned(),
                required_model: "V4 and later image models".to_owned(),
            }
            .into());
        }
        let normalized = params.base.normalize()?;
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(&normalized)?;
        self.send_stream_prepared(normalized.prepare()?, params.stream)
            .await
    }

    /// Starts a stream using an image-prepared request and its fixed seed.
    ///
    /// # Errors
    ///
    /// Returns capability, token-limit, transport or stream setup errors.
    pub async fn generate_stream_prepared(
        &self,
        prepared: PreparedGeneration,
        mode: StreamMode,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        if !prepared
            .normalized()
            .request()
            .descriptor()
            .support()
            .streaming
        {
            return Err(InvalidRequest::UnsupportedModelFeature {
                feature: "generate_stream".to_owned(),
                required_model: "V4 and later image models".to_owned(),
            }
            .into());
        }
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(prepared.normalized())?;
        self.send_stream_prepared(prepared, mode).await
    }

    async fn send_stream_prepared(
        &self,
        prepared: PreparedGeneration,
        mode: StreamMode,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        let (params, base, resolved_seed) = prepared.into_parts();
        let request = StreamImageGenerationRequestBody {
            action: base.action,
            input: base.input,
            model: base.model,
            use_new_shared_trial: base.use_new_shared_trial,
            parameters: ImageStreamParameters {
                base: base.parameters,
                stream: mode.as_str().to_owned(),
            },
        };
        let endpoint = self.image_endpoint("/ai/generate-image-stream");

        let observation = self
            .begin_generation_observation(&params, crate::ObservedOperation::GenerateImageStream)
            .await;
        match crate::protocol::post_sse(&*self.transport, endpoint.as_str(), &request).await {
            Ok(stream) => {
                let observation = self.stream_observation(observation);
                Ok(GenerateImageStream::new(resolved_seed, stream, observation))
            }
            Err(error) => {
                self.finish_observation_failed(observation, &error).await;
                Err(error)
            }
        }
    }

    #[cfg(feature = "tokenizer")]
    fn check_prompt_tokens(&self, params: &crate::NormalizedGeneration) -> Result<(), BridgeError> {
        let Some(tokenizers) = &self.tokenizers else {
            return Ok(());
        };
        for (field, count) in params.count_tokens(tokenizers)?.over_limit() {
            let error = InvalidRequest::PromptTokenLimitExceeded {
                field: field.label(),
                used: count.used,
                limit: count.limit,
            };
            if params.request().strict_mode {
                return Err(error.into());
            }
            log::warn!("{error}");
        }
        Ok(())
    }
}
