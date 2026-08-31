//! Target contracts, not snapshots of known bugs. Ignored tests name their remaining owner.
mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::StreamExt;
use novelai_bridge::{
    Character, CharacterPosition, Client, CreditEvent, CreditObserver, DecodeError, DecodeTarget,
    GenerateImageRequest, GenerateImageStreamRequest, ImageSize, Model, RequestOutcome,
};

use support::MockTransport;

#[derive(Default)]
struct Observer(Mutex<Vec<CreditEvent>>);

#[async_trait]
impl CreditObserver for Observer {
    async fn observe(&self, event: CreditEvent) {
        self.0.lock().unwrap().push(event);
    }
}

#[test]
#[ignore = "model descriptor: V5 inherits incompatible reference-inpainting support"]
fn reference_inpainting_requires_reference_support() {
    for model in [Model::NaiDiffusion5Full, Model::NaiDiffusion5Curated] {
        let caps = model.capabilities();
        assert!(!caps.supports_character_reference_inpainting || caps.supports_character_reference);
    }
}

#[tokio::test]
#[ignore = "normalization: public estimate must match the effective request"]
async fn public_estimate_matches_planned_estimate_after_size_normalization() {
    let transport = MockTransport::default();
    let observer = Arc::new(Observer::default());
    let client = Client::with_transport(transport.clone()).with_credit_observer(observer.clone());
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 2048,
            height: 2048,
        })
        .build();
    let estimate = client.estimate_anlas_cost(&request);
    client.generate(request).await.unwrap();
    let events = observer.0.lock().unwrap();
    let planned = events
        .iter()
        .find_map(|event| match event {
            CreditEvent::RequestPlanned { estimate, .. } => Some(estimate.total()),
            _ => None,
        })
        .unwrap();
    drop(events);
    assert_eq!(transport.last_body().unwrap()["parameters"]["width"], 1600);
    assert_eq!(estimate.total, planned);
}

#[tokio::test]
#[ignore = "normalization: disabled character fields must not reject generation"]
async fn disabled_empty_characters_do_not_participate_in_validation() {
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full)
        .characters(vec![Character {
            prompt: String::new(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: false,
        }])
        .build();
    let transport = MockTransport::default();
    Client::with_transport(transport.clone())
        .generate(request)
        .await
        .unwrap();
    assert_eq!(
        transport.last_body().unwrap()["parameters"]["characterPrompts"],
        serde_json::json!([])
    );
}

#[tokio::test]
#[ignore = "normalization: only the first max_characters enabled entries may be sent"]
async fn normalized_characters_respect_the_model_limit() {
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full)
        .characters(
            (0..7)
                .map(|index| Character {
                    prompt: format!("character {index}"),
                    negative_prompt: None,
                    position: CharacterPosition::default(),
                    enabled: true,
                })
                .collect(),
        )
        .build();
    let transport = MockTransport::default();
    Client::with_transport(transport.clone())
        .generate(request)
        .await
        .unwrap();
    assert_eq!(
        transport.last_body().unwrap()["parameters"]["characterPrompts"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

#[tokio::test]
#[ignore = "stream lifecycle: connection setup is not generation completion"]
async fn stream_failure_is_observed_as_failure_not_success() {
    let transport = MockTransport::default();
    transport.set_sse_error(
        DecodeError::new(
            DecodeTarget::StreamChunk,
            Box::new(std::io::Error::other("broken stream")),
        )
        .into(),
    );
    let observer = Arc::new(Observer::default());
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());
    let mut stream = client
        .generate_stream(GenerateImageStreamRequest {
            base: GenerateImageRequest::builder("test", Model::NaiDiffusion45Full).build(),
            ..GenerateImageStreamRequest::default()
        })
        .await
        .unwrap();
    assert!(stream.next().await.unwrap().is_err());
    let _ = stream.next().await;
    let events = observer.0.lock().unwrap();
    assert!(!events.iter().any(|event| matches!(
        event,
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Succeeded { .. },
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Failed { .. },
            ..
        }
    )));
    drop(events);
}

#[tokio::test]
#[ignore = "response decoding: non-image raw bytes must not be a generated image"]
async fn non_image_response_is_rejected() {
    let client =
        Client::with_transport(MockTransport::with_bytes_response(b"not an image".to_vec()));
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full).build();
    assert!(client.generate(request).await.is_err());
}
