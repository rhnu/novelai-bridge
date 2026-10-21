use tracing::Instrument;

use super::{Client, diagnostics::report_failure};
use crate::{BridgeError, EncodeVibeRequest, PreparedVibe, Transport, VibeEncoding};

impl<T: Transport + ?Sized> Client<T> {
    /// Prepares an image and encodes its vibe representation.
    ///
    /// # Errors
    /// Returns preparation, transport or response errors.
    pub async fn encode_vibe(
        &self,
        request: EncodeVibeRequest,
    ) -> Result<VibeEncoding, BridgeError> {
        let prepared = tracing::debug_span!(
            "novelai.prepare",
            operation = "encode_vibe",
            model = request.model.as_str()
        )
        .in_scope(|| request.prepare().inspect_err(report_failure))?;
        self.encode_vibe_prepared(prepared).await
    }

    /// Sends an already prepared vibe operation without preparing its source image again.
    ///
    /// # Errors
    /// Returns request encoding, transport or response errors.
    pub async fn encode_vibe_prepared(
        &self,
        prepared: PreparedVibe,
    ) -> Result<VibeEncoding, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("model", prepared.model().as_str());
            let body = prepared.encode().inspect_err(report_failure)?;
            let observation = self.observe_vibe(&prepared, request_id).await;
            self.execute_encoded(
                crate::protocol::ApiEndpoint::EncodeVibe,
                body,
                observation,
                |response| VibeEncoding::from_bytes(response.body).map(|encoding| (encoding, 0)),
            )
            .await
        }
        .instrument(Self::request_span("encode_vibe", request_id))
        .await
    }
}
