use std::time::SystemTime;

use crate::{BridgeError, SubscriptionInfo, Transport, subscription::SubscriptionResponse};

use super::Client;

impl<T: Transport + ?Sized> Client<T> {
    /// Fetches the current subscription summary for the configured account.
    ///
    /// Reports the result to the attached [`crate::CreditObserver`], if any.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when the transport request fails or the response cannot be decoded.
    pub async fn get_subscription(&self) -> Result<SubscriptionInfo, BridgeError> {
        let endpoint = self.image_endpoint(crate::protocol::Endpoint::Subscription);
        let raw = crate::protocol::get_json::<_, SubscriptionResponse>(&*self.transport, endpoint.as_str()).await?;
        let info = raw.into_info(SystemTime::now())?;
        *self.account.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(info.clone());
        #[cfg(feature = "credit-observer")]
        if let Some(observation) = &self.observation {
            observation.observe_subscription(&info).await;
        }
        Ok(info)
    }
}
