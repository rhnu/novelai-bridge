use super::Client;
use crate::{BridgeError, GeneratedImage, RunDirectorToolRequest, Transport};

impl<T: Transport + ?Sized> Client<T> {
    /// Prepares and runs a director operation against an image.
    ///
    /// # Errors
    /// Returns preparation, transport or response errors.
    pub async fn run_director_tool(&self, request: RunDirectorToolRequest) -> Result<GeneratedImage, BridgeError> {
        let prepared = request.prepare()?;
        let body = prepared.encode()?;
        let observation = self.observe_director(&prepared).await;
        self.execute_encoded(crate::protocol::Endpoint::AugmentImage, body, observation, |payload| {
            crate::image::decode_first_generated_image(payload).map(|image| (image, 1))
        }).await
    }
}
