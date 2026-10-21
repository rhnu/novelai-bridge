use std::{collections::VecDeque, sync::{Arc, Mutex}};

use async_trait::async_trait;

use super::Client;
use crate::{BridgeError, Transport, TransportError, TransportRequest, TransportResponse};

struct Ordinary {
    responses: Mutex<VecDeque<Vec<u8>>>,
}

#[async_trait]
impl Transport for Ordinary {
    async fn execute(&self, _: TransportRequest) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse {
            status: 200,
            headers: Vec::new(),
            body: self.responses.lock().unwrap().pop_front().unwrap(),
        })
    }
}

#[test]
fn cloning_a_client_does_not_require_a_cloneable_transport() {
    let transport: Arc<dyn Transport> = Arc::new(Ordinary { responses: Mutex::new(VecDeque::new()) });
    let client = Client::new(transport, Default::default());
    let cloned = client.clone();
    assert!(Arc::ptr_eq(&client.transport, &cloned.transport));
    assert!(Arc::ptr_eq(&client.account, &cloned.account));
}

#[tokio::test]
async fn account_cache_is_shared_and_an_invalid_response_does_not_replace_it() {
    let transport = Arc::new(Ordinary { responses: Mutex::new(VecDeque::from([
        br#"{"tier":3,"active":true,"trainingStepsLeft":{"fixedTrainingStepsLeft":55}}"#.to_vec(),
        b"{}".to_vec(),
    ])) });
    let client = Client::new(transport, Default::default());
    let cloned = client.clone();
    assert!(client.last_subscription().is_none());
    let subscription = client.get_subscription().await.unwrap();
    assert_eq!(cloned.last_subscription(), Some(subscription.clone()));
    assert!(matches!(cloned.get_subscription().await, Err(BridgeError::Decode(_))));
    assert_eq!(client.last_subscription(), Some(subscription));
}

#[cfg(feature = "credit-observer")]
#[derive(Default)]
struct Observer(Mutex<Vec<crate::CreditEvent>>);

#[cfg(feature = "credit-observer")]
#[async_trait]
impl crate::CreditObserver for Observer {
    async fn observe(&self, event: crate::CreditEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[cfg(feature = "credit-observer")]
#[tokio::test]
async fn common_execution_settles_decode_failure_and_success_with_distinct_identifiers() {
    let observer = Arc::new(Observer::default());
    let transport = Arc::new(Ordinary { responses: Mutex::new(VecDeque::from([
        b"invalid".to_vec(), b"payload".to_vec(),
    ])) });
    let client = Client::new(transport, Default::default()).with_credit_observer(observer.clone());
    let normalized = crate::GenerateImageRequest::builder("private prompt", crate::Model::NaiDiffusion45Full)
        .build().normalize().unwrap();
    let pending = client.observe_generation(&normalized, false).await;
    let result: Result<(), BridgeError> = client.execute_encoded(
        crate::protocol::Endpoint::GenerateImage, Vec::new(), pending,
        |_| Err(crate::DecodeError::new(crate::DecodeTarget::ImageResponse, "invalid image".into()).into()),
    ).await;
    assert!(matches!(result, Err(BridgeError::Decode(_))));
    let pending = client.observe_generation(&normalized, false).await;
    let result = client.execute_encoded(
        crate::protocol::Endpoint::GenerateImage, Vec::new(), pending, |_| Ok(((), 2)),
    ).await;
    assert!(result.is_ok());
    let events = observer.0.lock().unwrap();
    assert_eq!(events.len(), 4);
    assert!(matches!(&events[1], crate::CreditEvent::RequestSettled {
        fingerprint, outcome: crate::RequestOutcome::Failed { failure: crate::RequestFailure::Decode },
    } if fingerprint.request_id == 1));
    assert!(matches!(&events[3], crate::CreditEvent::RequestSettled {
        fingerprint, outcome: crate::RequestOutcome::Succeeded { image_count: 2 },
    } if fingerprint.request_id == 2));
    assert!(!format!("{events:?}").contains("private prompt"));
}
