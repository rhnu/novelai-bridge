use super::Client;
use crate::{BridgeError, EncodeVibeRequest, Transport, VibeEncoding};

impl<T: Transport + ?Sized> Client<T> {
    /// Prepares an image and encodes its vibe representation.
    ///
    /// # Errors
    /// Returns preparation, transport or response errors.
    pub async fn encode_vibe(&self, request: EncodeVibeRequest) -> Result<VibeEncoding, BridgeError> {
        let prepared = request.prepare()?;
        let body = prepared.encode()?;
        let observation = self.observe_vibe(&prepared).await;
        self.execute_encoded(crate::protocol::Endpoint::EncodeVibe, body, observation, |payload| {
            VibeEncoding::from_bytes(payload).map(|encoding| (encoding, 0))
        }).await
    }
}
