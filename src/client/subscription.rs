use std::time::SystemTime;

use tracing::Instrument;

use super::Client;
use crate::{BridgeError, SubscriptionInfo, Transport, subscription::SubscriptionResponse};

impl<T: Transport + ?Sized> Client<T> {
    /// Fetches the current subscription summary for the configured account.
    ///
    /// Reports the result to the attached [`crate::CreditObserver`], if any.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when the transport request fails or the response cannot be decoded.
    pub async fn get_subscription(&self) -> Result<SubscriptionInfo, BridgeError> {
        let request_id = self.next_request_id();
        async {
            tracing::Span::current().record(
                "endpoint",
                crate::protocol::ApiEndpoint::Subscription.path(),
            );
            let request = crate::protocol::ApiEndpoint::Subscription
                .request(&self.config.image_base_url, None);
            let raw = crate::protocol::get_json::<_, SubscriptionResponse>(
                &*self.transport,
                request,
                self.config.response_limits,
            )
            .await
            .inspect_err(super::diagnostics::report_failure)?;
            let info = raw
                .into_info(SystemTime::now())
                .map_err(BridgeError::from)
                .inspect_err(super::diagnostics::report_failure)?;
            *self
                .account
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(info.clone());
            if let Some(observation) = &self.observation {
                observation.observe_subscription(&info).await;
            }
            tracing::debug!(
                tier = info.tier,
                subscription_active = info.subscription_active,
                "subscription sampled"
            );
            Ok(info)
        }
        .instrument(Self::request_span("subscription", request_id))
        .await
    }
}
