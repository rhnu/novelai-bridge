use super::{Client, observe::PendingObservation};
use crate::{BridgeError, Transport};

impl<T: Transport + ?Sized> Client<T> {
    pub(super) async fn execute_encoded<ResultValue>(
        &self,
        endpoint: crate::protocol::ApiEndpoint,
        body: crate::image::request_body::RequestBody,
        accept: Option<&str>,
        observation: PendingObservation,
        decode: impl FnOnce(
            crate::protocol::BufferedResponse,
        ) -> Result<(ResultValue, u32), BridgeError>,
    ) -> Result<ResultValue, BridgeError> {
        let started = std::time::Instant::now();
        tracing::Span::current().record("endpoint", endpoint.path());
        let mut request = endpoint.request(&self.config.image_base_url, None);
        if let Some(accept) = accept {
            request.accept = Some(accept.to_owned());
        }
        let url = request.endpoint.clone();
        let result = match self
            .execute_image_request(request, body, endpoint == crate::ApiEndpoint::GenerateImage)
            .await
        {
            Ok(response) => {
                crate::protocol::buffer_response(response, &url, self.config.response_limits)
                    .await
                    .and_then(decode)
            },
            Err(error) => Err(error),
        };
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
