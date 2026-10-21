use tracing::Instrument;

use super::{Client, diagnostics::report_failure};
use crate::{
    BridgeError, PreparedDirector, RunDirectorToolRequest, RunDirectorToolResult, Transport,
};

impl<T: Transport + ?Sized> Client<T> {
    /// Prepares and runs a director operation against an image.
    ///
    /// # Errors
    /// Returns preparation, transport or response errors.
    pub async fn run_director_tool(
        &self,
        request: RunDirectorToolRequest,
    ) -> Result<RunDirectorToolResult, BridgeError> {
        let prepared = tracing::debug_span!(
            "novelai.prepare",
            operation = "director",
            tool = request.command.wire_name()
        )
        .in_scope(|| request.prepare().inspect_err(report_failure))?;
        self.run_director_tool_prepared(prepared).await
    }

    /// Sends an already prepared director operation without preparing the source image again.
    ///
    /// # Errors
    /// Returns request encoding, transport or response errors.
    pub async fn run_director_tool_prepared(
        &self,
        mut prepared: PreparedDirector,
    ) -> Result<RunDirectorToolResult, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("tool", prepared.command().wire_name());
            let body = if self.needs_image_projection(false) {
                prepared.take_request_body()
            } else {
                prepared
                    .encode()
                    .map(crate::image::request_body::RequestBody::Json)
            }
            .inspect_err(report_failure)?;
            let observation = self.observe_director(&prepared, request_id).await;
            drop(prepared);
            self.execute_encoded(
                crate::protocol::ApiEndpoint::AugmentImage,
                body,
                None,
                observation,
                |response| {
                    let images = crate::image::decode_generated_image_response(response)?;
                    let image_count = u32::try_from(images.len()).unwrap_or(u32::MAX);
                    Ok((RunDirectorToolResult { images }, image_count))
                },
            )
            .await
        }
        .instrument(Self::request_span("director", request_id))
        .await
    }
}

#[cfg(test)]
mod tests;
