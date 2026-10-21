use tracing::Instrument;

use super::{Client, diagnostics::report_failure};
#[cfg(feature = "tokenizer")]
use crate::InvalidRequest;
use crate::{
    BridgeError, GenerateImageRequest, GenerateImageResult, GenerateImageStream,
    GenerateImageStreamRequest, GenerationDelivery, PreparedGeneration, StreamMode, Transport,
    decode_generated_image_response,
};

impl<T: Transport + ?Sized> Client<T> {
    /// Normalizes, prepares and sends an image-generation request.
    ///
    /// # Errors
    /// Returns local preparation, transport, or response errors.
    pub async fn generate(
        &self,
        request: GenerateImageRequest,
    ) -> Result<GenerateImageResult, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("model", request.core.model.as_str());
            let normalized = request.normalize().inspect_err(report_failure)?;
            #[cfg(feature = "tokenizer")]
            self.check_prompt_tokens(&normalized)
                .inspect_err(report_failure)?;
            let prepared = normalized.prepare().inspect_err(report_failure)?;
            self.send_prepared(prepared, request_id).await
        }
        .instrument(Self::request_span("generate", request_id))
        .await
    }

    /// Sends a prepared request without resolving its seed or preprocessing images again.
    ///
    /// # Errors
    /// Returns token-limit, transport or response errors.
    pub async fn generate_prepared(
        &self,
        prepared: PreparedGeneration,
    ) -> Result<GenerateImageResult, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current()
                .record("model", prepared.normalized().request().core.model.as_str());
            #[cfg(feature = "tokenizer")]
            self.check_prompt_tokens(prepared.normalized())
                .inspect_err(report_failure)?;
            self.send_prepared(prepared, request_id).await
        }
        .instrument(Self::request_span("generate", request_id))
        .await
    }

    async fn send_prepared(
        &self,
        mut prepared: PreparedGeneration,
        request_id: u64,
    ) -> Result<GenerateImageResult, BridgeError> {
        let body = if self.needs_image_projection(true) {
            prepared.take_request_body(GenerationDelivery::Images)
        } else {
            prepared
                .encode(GenerationDelivery::Images)
                .map(crate::image::request_body::RequestBody::Json)
        }
        .inspect_err(report_failure)?;
        let resolved_seed = prepared.resolved_seed();
        let observation = self
            .observe_generation(
                prepared.normalized(),
                GenerationDelivery::Images,
                request_id,
            )
            .await;
        let accept = prepared
            .normalized()
            .request()
            .output
            .response_format
            .accept();
        drop(prepared);
        self.execute_encoded(
            crate::protocol::ApiEndpoint::GenerateImage,
            body,
            Some(accept),
            observation,
            |payload| {
                let images = decode_generated_image_response(payload)?;
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
    pub async fn generate_stream(
        &self,
        request: GenerateImageStreamRequest,
    ) -> Result<GenerateImageStream, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("model", request.base.core.model.as_str());
            let normalized = request.base.normalize().inspect_err(report_failure)?;
            #[cfg(feature = "tokenizer")]
            self.check_prompt_tokens(&normalized)
                .inspect_err(report_failure)?;
            let prepared = normalized.prepare().inspect_err(report_failure)?;
            self.send_stream_prepared(prepared, request.stream, request_id)
                .await
        }
        .instrument(Self::request_span("generate_stream", request_id))
        .await
    }

    /// Starts a stream without resolving its seed or preprocessing images again.
    ///
    /// # Errors
    /// Returns capability, token-limit, transport or stream setup errors.
    pub async fn generate_stream_prepared(
        &self,
        prepared: PreparedGeneration,
        mode: StreamMode,
    ) -> Result<GenerateImageStream, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current()
                .record("model", prepared.normalized().request().core.model.as_str());
            #[cfg(feature = "tokenizer")]
            self.check_prompt_tokens(prepared.normalized())
                .inspect_err(report_failure)?;
            self.send_stream_prepared(prepared, mode, request_id).await
        }
        .instrument(Self::request_span("generate_stream", request_id))
        .await
    }

    async fn send_stream_prepared(
        &self,
        mut prepared: PreparedGeneration,
        mode: StreamMode,
        request_id: u64,
    ) -> Result<GenerateImageStream, BridgeError> {
        tracing::Span::current().record(
            "endpoint",
            crate::protocol::ApiEndpoint::GenerateImageStream.path(),
        );
        let body = if self.needs_image_projection(true) {
            prepared.take_request_body(GenerationDelivery::Stream(mode))
        } else {
            prepared
                .encode(GenerationDelivery::Stream(mode))
                .map(crate::image::request_body::RequestBody::Json)
        }
        .inspect_err(report_failure)?;
        let resolved_seed = prepared.resolved_seed();
        let observation = self
            .observe_generation(
                prepared.normalized(),
                GenerationDelivery::Stream(mode),
                request_id,
            )
            .await;
        let n_samples = prepared.normalized().request().core.n_samples;
        drop(prepared);
        let mut request = crate::protocol::ApiEndpoint::GenerateImageStream
            .request(&self.config.image_base_url, None);
        if mode == StreamMode::MessagePack {
            request.accept = None;
        }
        let url = request.endpoint.clone();
        let result = match self.execute_image_request(request, body, true).await {
            Ok(response) => {
                crate::protocol::decode_stream_response(
                    response,
                    &url,
                    mode,
                    self.config.response_limits,
                )
                .await
            },
            Err(error) => Err(error),
        };
        match result {
            Ok(stream) => {
                tracing::debug!("image stream opened");
                Ok(GenerateImageStream::new(
                    resolved_seed,
                    n_samples,
                    stream,
                    observation,
                ))
            },
            Err(error) => {
                report_failure(&error);
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
                field: field.generation_field().into(),
                used: count.used,
                limit: count.limit,
            };
            if params.request().policy.is_reject() {
                return Err(error.into());
            }
            tracing::warn!(field = %field.generation_field(), used = count.used, limit = count.limit, model = %params.request().core.model.as_str(), "prompt exceeds token limit");
        }
        Ok(())
    }
}
