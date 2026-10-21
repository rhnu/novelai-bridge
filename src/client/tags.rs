use tracing::Instrument;

use super::{Client, diagnostics::report_failure};
use crate::{ApiEndpoint, BridgeError, SuggestTagsRequest, SuggestTagsResult, Transport};

impl<T: Transport + ?Sized> Client<T> {
    /// Queries tag suggestions using a URL-encoded prefix and language.
    ///
    /// # Errors
    /// Returns transport, API status or JSON response failures.
    pub async fn suggest_tags(
        &self,
        query: SuggestTagsRequest,
    ) -> Result<SuggestTagsResult, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record("model", query.model.as_str());
            tracing::Span::current().record("endpoint", ApiEndpoint::SuggestTags.path());
            let mut request = ApiEndpoint::SuggestTags.request(&self.config.image_base_url, None);
            request.endpoint.push('?');
            request.endpoint.push_str(&query.query_string());
            // Some official successful responses carry JSON under text/plain.
            let result: SuggestTagsResult =
                crate::protocol::get_json(&*self.transport, request, self.config.response_limits)
                    .await
                    .inspect_err(report_failure)?;
            tracing::debug!(tag_count = result.len(), "tag suggestions received");
            Ok(result)
        }
        .instrument(Self::request_span("suggest_tags", request_id))
        .await
    }
}

#[cfg(test)]
mod tests;
