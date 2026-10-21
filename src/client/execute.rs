use super::{Client, observe::PendingObservation};
use crate::{BridgeError, Transport};

impl<T: Transport + ?Sized> Client<T> {
    pub(super) async fn execute_encoded<ResultValue>(
        &self,
        endpoint: crate::protocol::ApiEndpoint,
        body: Vec<u8>,
        observation: PendingObservation,
        decode: impl FnOnce(Vec<u8>) -> Result<(ResultValue, u32), BridgeError>,
    ) -> Result<ResultValue, BridgeError> {
        let started = std::time::Instant::now();
        tracing::Span::current().record("endpoint", endpoint.path());
        tracing::debug!(body_bytes = body.len(), "sending request");
        let request = endpoint.request(&self.config.image_base_url, Some(body));
        let result = crate::protocol::post_encoded(&*self.transport, request)
            .await
            .and_then(decode);
        match result {
            Ok((value, image_count)) => {
                tracing::debug!(
                    image_count,
                    elapsed_secs = started.elapsed().as_secs_f64(),
                    outcome = "succeeded",
                    "request completed"
                );
                observation.succeed(image_count).await;
                Ok(value)
            },
            Err(error) => {
                tracing::debug!(
                    elapsed_secs = started.elapsed().as_secs_f64(),
                    outcome = "failed",
                    "request completed"
                );
                super::diagnostics::report_failure(&error);
                observation.fail(&error).await;
                Err(error)
            },
        }
    }
}
