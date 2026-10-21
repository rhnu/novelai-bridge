use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use super::{Client, ClientConfig};
use crate::{BridgeError, Transport, TransportError, TransportRequest, TransportResponse};

struct Ordinary {
    responses: Mutex<VecDeque<Vec<u8>>>,
    requests: Mutex<Vec<TransportRequest>>,
}

#[async_trait]
impl Transport for Ordinary {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        self.requests.lock().unwrap().push(request);
        Ok(TransportResponse {
            body_truncated: false,
            status: 200,
            headers: Vec::new(),
            body: self
                .responses
                .lock()
                .expect("test response mutex is not poisoned")
                .pop_front()
                .expect("one response was configured"),
        })
    }
}

#[test]
fn cloning_a_client_does_not_require_a_cloneable_transport() {
    let transport: Arc<dyn Transport> = Arc::new(Ordinary {
        requests: Mutex::default(),
        responses: Mutex::new(VecDeque::new()),
    });
    let client = Client::new(transport, ClientConfig::default());
    let cloned = client.clone();
    assert!(Arc::ptr_eq(&client.transport, &cloned.transport));
    assert!(Arc::ptr_eq(&client.account, &cloned.account));
}

#[tokio::test]
async fn account_cache_is_shared_and_an_invalid_response_does_not_replace_it() {
    let transport = Arc::new(Ordinary {
        requests: Mutex::default(),
        responses: Mutex::new(VecDeque::from([
            br#"{"tier":3,"active":true,"trainingStepsLeft":{"fixedTrainingStepsLeft":55}}"#
                .to_vec(),
            b"{}".to_vec(),
        ])),
    });
    let client = Client::new(transport, ClientConfig::default());
    let cloned = client.clone();
    assert!(client.last_subscription().is_none());
    let subscription = client.get_subscription().await.unwrap();
    assert_eq!(cloned.last_subscription(), Some(subscription.clone()));
    assert!(matches!(
        cloned.get_subscription().await,
        Err(BridgeError::Decode(_))
    ));
    assert_eq!(client.last_subscription(), Some(subscription));
}

#[cfg(feature = "credit-observer")]
#[derive(Default)]
struct Observer(Mutex<Vec<crate::CreditEvent>>);

#[cfg(feature = "credit-observer")]
#[async_trait]
impl crate::CreditObserver for Observer {
    async fn observe(&self, event: crate::CreditEvent) {
        self.0
            .lock()
            .expect("test event mutex is not poisoned")
            .push(event);
    }
}

#[cfg(feature = "credit-observer")]
#[tokio::test]
async fn common_execution_settles_decode_failure_and_success_with_distinct_identifiers() {
    let observer = Arc::new(Observer::default());
    let transport = Arc::new(Ordinary {
        requests: Mutex::default(),
        responses: Mutex::new(VecDeque::from([b"invalid".to_vec(), b"payload".to_vec()])),
    });
    let client =
        Client::new(transport, ClientConfig::default())
            .with_credit_observer(Arc::<Observer>::clone(&observer));
    let normalized =
        crate::GenerateImageRequest::builder("private prompt", crate::Model::NaiDiffusion45Full)
            .build()
            .normalize()
            .unwrap();
    let pending = client
        .observe_generation(
            &normalized,
            crate::GenerationDelivery::Images,
            client.next_request_id(),
        )
        .await;
    let result: Result<(), BridgeError> = client
        .execute_encoded(
            crate::protocol::ApiEndpoint::GenerateImage,
            Vec::new(),
            pending,
            |_| {
                Err(crate::DecodeError::new(
                    crate::DecodeTarget::ImageResponse,
                    "invalid image".into(),
                )
                .into())
            },
        )
        .await;
    assert!(matches!(result, Err(BridgeError::Decode(_))));
    let pending = client
        .observe_generation(
            &normalized,
            crate::GenerationDelivery::Images,
            client.next_request_id(),
        )
        .await;
    let result = client
        .execute_encoded(
            crate::protocol::ApiEndpoint::GenerateImage,
            Vec::new(),
            pending,
            |_| Ok(((), 2)),
        )
        .await;
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
    drop(events);
}

fn source_image() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn director_prepared_and_raw_calls_share_transport_and_decode_behavior() {
    let png = source_image();
    let transport = Arc::new(Ordinary {
        requests: Mutex::default(),
        responses: Mutex::new(VecDeque::from([png.clone(), png.clone()])),
    });
    let client = Client::new(Arc::<Ordinary>::clone(&transport), ClientConfig::default());
    let request = crate::RunDirectorToolRequest {
        image: crate::EncodedImage::from_bytes(png.clone()).unwrap(),
        command: crate::DirectorCommand::Lineart,
        policy: crate::ValidationPolicy::Reject,
    };
    let prepared = request.clone().prepare().unwrap();
    let raw_result = client.run_director_tool(request).await.unwrap();
    let prepared_result = client.run_director_tool_prepared(prepared).await.unwrap();
    assert_eq!(raw_result.bytes(), png);
    assert_eq!(raw_result, prepared_result);
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
}

#[tokio::test]
async fn vibe_prepared_and_raw_calls_share_transport_and_decode_behavior() {
    let transport = Arc::new(Ordinary {
        requests: Mutex::default(),
        responses: Mutex::new(VecDeque::from([b"opaque".to_vec(), b"opaque".to_vec()])),
    });
    let client = Client::new(Arc::<Ordinary>::clone(&transport), ClientConfig::default());
    let request = crate::EncodeVibeRequest {
        image: crate::EncodedImage::from_bytes(source_image()).unwrap(),
        information_extracted: 1.0,
        model: crate::Model::NaiDiffusion45Full,
        policy: crate::ValidationPolicy::Reject,
    };
    let prepared = request.clone().prepare().unwrap();
    let raw_result = client.encode_vibe(request).await.unwrap();
    let prepared_result = client.encode_vibe_prepared(prepared).await.unwrap();
    assert_eq!(raw_result.as_bytes(), b"opaque");
    assert_eq!(raw_result, prepared_result);
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
}
