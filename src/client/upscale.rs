use tracing::Instrument;

use super::{Client, diagnostics::report_failure};
use crate::{BridgeError, PreparedUpscale, Transport, UpscaleImageRequest, UpscaleImageResult};

impl<T: Transport + ?Sized> Client<T> {
    /// Prepares an image and sends it to the standalone upscaler.
    ///
    /// # Errors
    /// Returns image validation, transport or response decoding failures.
    pub async fn upscale(
        &self,
        request: UpscaleImageRequest,
    ) -> Result<UpscaleImageResult, BridgeError> {
        let prepared = tracing::debug_span!("novelai.prepare", operation = "upscale")
            .in_scope(|| request.prepare().inspect_err(report_failure))?;
        self.upscale_prepared(prepared).await
    }

    /// Sends a prepared upscale payload without preparing the source again.
    ///
    /// # Errors
    /// Returns request encoding, transport or response decoding failures.
    pub async fn upscale_prepared(
        &self,
        prepared: PreparedUpscale,
    ) -> Result<UpscaleImageResult, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("model", prepared.model().as_str());
            let body = prepared.encode().inspect_err(report_failure)?;
            let observation = self.observe_upscale(&prepared, request_id).await;
            self.execute_encoded(
                crate::ApiEndpoint::UpscaleImage,
                body,
                observation,
                |response| {
                    let images = crate::decode_generated_images(response.body)?;
                    let image_count = u32::try_from(images.len()).unwrap_or(u32::MAX);
                    Ok((UpscaleImageResult { images }, image_count))
                },
            )
            .await
        }
        .instrument(Self::request_span("upscale", request_id))
        .await
    }
}

#[cfg(test)]
mod tests;
