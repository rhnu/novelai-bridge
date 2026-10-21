use super::Client;
#[cfg(feature = "tokenizer")]
use crate::InvalidRequest;
use crate::{
    BridgeError, GenerateImageRequest, GenerateImageResult, GenerationDelivery, PreparedGeneration,
    Transport, decode_generated_images,
};
#[cfg(feature = "streaming")]
use crate::{GenerateImageStream, GenerateImageStreamRequest, StreamMode, StreamingTransport};

impl<T: Transport + ?Sized> Client<T> {
    /// Normalizes, prepares and sends an image-generation request.
    ///
    /// # Errors
    /// Returns local preparation, transport, or response errors.
    pub async fn generate(
        &self,
        request: GenerateImageRequest,
    ) -> Result<GenerateImageResult, BridgeError> {
        let normalized = request.normalize()?;
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(&normalized)?;
        self.send_prepared(normalized.prepare()?).await
    }

    /// Sends a prepared request without resolving its seed or preprocessing images again.
    ///
    /// # Errors
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
        let body = prepared.encode(GenerationDelivery::Images)?;
        let resolved_seed = prepared.resolved_seed();
        let observation = self.observe_generation(prepared.normalized(), false).await;
        self.execute_encoded(
            crate::protocol::Endpoint::GenerateImage,
            body,
            observation,
            |payload| {
                let images = decode_generated_images(payload)?;
                let count = u32::try_from(images.len()).unwrap_or(u32::MAX);
                Ok((
                    GenerateImageResult {
                        resolved_seed,
                        images,
                    },
                    count,
                ))
            },
        )
        .await
    }

    /// Starts a prepared SSE generation stream.
    ///
    /// # Errors
    /// Returns local preparation, transport or stream setup errors.
    #[cfg(feature = "streaming")]
    pub async fn generate_stream(
        &self,
        request: GenerateImageStreamRequest,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        let normalized = request.base.normalize()?;
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(&normalized)?;
        self.send_stream_prepared(normalized.prepare()?, request.stream)
            .await
    }

    /// Starts a stream without resolving its seed or preprocessing images again.
    ///
    /// # Errors
    /// Returns capability, token-limit, transport or stream setup errors.
    #[cfg(feature = "streaming")]
    pub async fn generate_stream_prepared(
        &self,
        prepared: PreparedGeneration,
        mode: StreamMode,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        #[cfg(feature = "tokenizer")]
        self.check_prompt_tokens(prepared.normalized())?;
        self.send_stream_prepared(prepared, mode).await
    }

    #[cfg(feature = "streaming")]
    async fn send_stream_prepared(
        &self,
        prepared: PreparedGeneration,
        mode: StreamMode,
    ) -> Result<GenerateImageStream, BridgeError>
    where
        T: StreamingTransport,
    {
        let body = prepared.encode(GenerationDelivery::Stream(mode))?;
        let resolved_seed = prepared.resolved_seed();
        let observation = self.observe_generation(prepared.normalized(), true).await;
        let endpoint = self.image_endpoint(crate::protocol::Endpoint::GenerateImageStream);
        match crate::protocol::post_sse(&*self.transport, &endpoint, body).await {
            Ok(stream) => Ok(GenerateImageStream::new(resolved_seed, stream, observation)),
            Err(error) => {
                observation.fail(&error).await;
                Err(error)
            },
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
