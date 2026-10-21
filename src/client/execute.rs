use crate::{BridgeError, Transport};

use super::{Client, observe::PendingObservation};

impl<T: Transport + ?Sized> Client<T> {
    pub(super) async fn execute_encoded<ResultValue>(
        &self,
        endpoint: crate::protocol::Endpoint,
        body: Vec<u8>,
        observation: PendingObservation,
        decode: impl FnOnce(Vec<u8>) -> Result<(ResultValue, u32), BridgeError>,
    ) -> Result<ResultValue, BridgeError> {
        let endpoint = self.image_endpoint(endpoint);
        let result = crate::protocol::post_encoded(&*self.transport, &endpoint, body).await.and_then(decode);
        match result {
            Ok((value, image_count)) => {
                observation.succeed(image_count).await;
                Ok(value)
            }
            Err(error) => {
                observation.fail(&error).await;
                Err(error)
            }
        }
    }
}
