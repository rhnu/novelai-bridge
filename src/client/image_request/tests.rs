use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::*;
use crate::{ApiEndpoint, ClientConfig, DEFAULT_IMAGE_BASE, TransportError};

enum Reply {
    Success,
    InvalidKey,
    Other(u16),
    Network,
}

struct Mock {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<TransportRequest>>,
}

#[async_trait]
impl Transport for Mock {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        let body: Value = serde_json::from_slice(request.body.as_deref().unwrap()).unwrap();
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("expected request");
        Ok(match reply {
            Reply::Success => TransportResponse::from_bytes(
                200,
                vec![],
                crate::generation::test_support::image().as_bytes().to_vec(),
            ),
            Reply::InvalidKey => TransportResponse::from_bytes(
                400,
                vec![],
                serde_json::to_vec(&json!({
                    "message": "INVALID_CACHE_KEYS",
                    "details": {"invalidKeys": [body["parameters"]["image_cache_secret_key"]]}
                }))
                .unwrap(),
            ),
            Reply::Other(status) => {
                TransportResponse::from_bytes(status, vec![], b"{\"message\":\"other\"}".to_vec())
            },
            Reply::Network => {
                return Err(TransportError::new(
                    crate::TransportOperation::SendRequest,
                    None,
                    "connection lost".into(),
                ));
            },
        })
    }
}

fn test_client(replies: Vec<Reply>, max_entries: usize) -> Client<Mock> {
    Client::new(
        Arc::new(Mock {
            replies: Mutex::new(replies.into()),
            requests: Mutex::default(),
        }),
        ClientConfig {
            image_cache: Some(ImageCacheConfig { max_entries }),
            ..ClientConfig::default()
        },
    )
}

fn request(data: &str) -> TransportRequest {
    ApiEndpoint::GenerateImage.request(DEFAULT_IMAGE_BASE, Some(serde_json::to_vec(&json!({
        "parameters": {"image": data, "mask":"BAUG", "reference_image_multiple":[data], "director_reference_images":[data]}
    })).unwrap()))
}

fn bodies(client: &Client<Mock>) -> Vec<Value> {
    client
        .transport
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|request| serde_json::from_slice(request.body.as_deref().unwrap()).unwrap())
        .collect()
}

#[tokio::test]
async fn clones_share_registration_and_invalid_keys_restore_only_matching_media_once() {
    let client = test_client(
        vec![
            Reply::Success,
            Reply::InvalidKey,
            Reply::Success,
            Reply::Success,
        ],
        16,
    );
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    client
        .clone()
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    let requests = bodies(&client);
    assert_eq!(requests.len(), 4);
    let first = &requests[0]["parameters"];
    assert_eq!(first["image"], "AQID");
    assert_eq!(first["reference_image_multiple_cached"][0]["data"], "AQID");
    assert!(first.get("reference_image_multiple").is_none());
    let second = &requests[1]["parameters"];
    assert!(second.get("image").is_none());
    assert!(
        second["reference_image_multiple_cached"][0]
            .get("data")
            .is_none()
    );
    let restored = &requests[2]["parameters"];
    assert_eq!(restored["image"], "AQID");
    assert_eq!(
        restored["reference_image_multiple_cached"][0]["data"],
        "AQID"
    );
    assert_eq!(
        restored["director_reference_images_cached"][0]["data"],
        "AQID"
    );
    assert!(restored.get("mask").is_none());
    assert_eq!(requests[3], requests[1]);
    let another = test_client(vec![Reply::Success], 16);
    another
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    assert_ne!(
        bodies(&another)[0]["parameters"]["image_cache_secret_key"],
        first["image_cache_secret_key"]
    );
}

#[tokio::test]
async fn network_other_statuses_and_unrestorable_keys_never_retry() {
    for reply in [
        Reply::Network,
        Reply::Other(500),
        Reply::Other(401),
        Reply::Other(400),
        Reply::InvalidKey,
    ] {
        let client = test_client(vec![reply], 16);
        let result = client.execute_image_request(request("AQID"), true).await;
        if let Ok(response) = result {
            assert!(!(200..300).contains(&response.status));
        }
        assert_eq!(bodies(&client).len(), 1);
    }
}

#[tokio::test]
async fn a_second_cache_rejection_is_terminal_and_invalidated_data_is_sent_next_time() {
    let client = test_client(
        vec![
            Reply::Success,
            Reply::InvalidKey,
            Reply::InvalidKey,
            Reply::Success,
        ],
        16,
    );
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    let response = client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    assert_eq!(response.status, 400);
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    let requests = bodies(&client);
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[3]["parameters"]["image"], "AQID");
}

#[tokio::test]
async fn bounded_registration_evicts_old_keys_without_retaining_image_payloads() {
    let client = test_client(vec![Reply::Success, Reply::Success, Reply::Success], 1);
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    client
        .execute_image_request(request("BwgJ"), true)
        .await
        .unwrap();
    client
        .execute_image_request(request("AQID"), true)
        .await
        .unwrap();
    assert_eq!(bodies(&client)[2]["parameters"]["image"], "AQID");
}

#[derive(Default)]
struct Observer(Mutex<Vec<crate::CreditEvent>>);

#[async_trait]
impl crate::CreditObserver for Observer {
    async fn observe(&self, event: crate::CreditEvent) { self.0.lock().unwrap().push(event); }
}

#[tokio::test]
async fn cache_recovery_stays_inside_one_credit_observation_lifecycle() {
    let observer = Arc::new(Observer::default());
    let client = test_client(vec![Reply::Success, Reply::InvalidKey, Reply::Success], 16)
        .with_credit_observer(Arc::<Observer>::clone(&observer));
    let request = crate::GenerateImageRequest::builder("apple", crate::Model::NaiDiffusion5Full)
        .size(crate::ImageSize {
            width: 256,
            height: 256,
        })
        .seed(42)
        .steps(2)
        .image_to_image(crate::ImageToImageInput::new(
            crate::generation::test_support::image(),
            0.7,
            0.0,
        ))
        .build();
    client.generate(request.clone()).await.unwrap();
    client.generate(request).await.unwrap();
    assert_eq!(bodies(&client).len(), 3);
    let events = observer.0.lock().unwrap();
    assert_eq!(events.len(), 4);
    assert!(matches!(
        &events[0],
        crate::CreditEvent::RequestPlanned { .. }
    ));
    assert!(matches!(
        &events[1],
        crate::CreditEvent::RequestSettled {
            outcome: crate::RequestOutcome::Succeeded { .. },
            ..
        }
    ));
    assert!(matches!(
        &events[2],
        crate::CreditEvent::RequestPlanned { .. }
    ));
    assert!(matches!(
        &events[3],
        crate::CreditEvent::RequestSettled {
            outcome: crate::RequestOutcome::Succeeded { .. },
            ..
        }
    ));
    drop(events);
}
