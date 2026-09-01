//! Target contracts, not snapshots of known bugs. Ignored tests name their remaining owner.
mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::StreamExt;
use novelai_bridge::{
    Character, CharacterPosition, Client, CreditEvent, CreditObserver, DecodeError, DecodeTarget,
    GenerateImageRequest, GenerateImageStreamRequest, ImageSize, ImageStreamChunk, Model,
    RequestOutcome,
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
fn reference_inpainting_requires_reference_support() {
    for model in [Model::NaiDiffusion5Full, Model::NaiDiffusion5Curated] {
        let caps = model.descriptor();
        assert!(caps.support().character_reference.is_none());
        for mode in [
            novelai_bridge::GenerationMode::TextToImage,
            novelai_bridge::GenerationMode::ImageToImage,
            novelai_bridge::GenerationMode::Inpainting,
        ] {
            assert!(
                !caps
                    .resolve_references(
                        mode,
                        novelai_bridge::ReferenceIntent {
                            vibe: false,
                            character: true,
                        }
                    )
                    .character
                    .is_available()
            );
        }
    }
}

#[tokio::test]
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
    let estimate = client.estimate_anlas_cost(&request).unwrap();
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
    assert_eq!(planned, 42);
}

#[tokio::test]
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

fn stream_chunk(sample: u32) -> ImageStreamChunk {
    ImageStreamChunk {
        event_type: "final".to_owned(),
        samp_ix: sample,
        step_ix: None,
        gen_id: 1,
        sigma: None,
        image: "image fragment".to_owned(),
    }
}

fn stream_request() -> GenerateImageStreamRequest {
    GenerateImageStreamRequest {
        base: GenerateImageRequest::builder("test", Model::NaiDiffusion45Full).build(),
        ..GenerateImageStreamRequest::default()
    }
}

#[tokio::test]
async fn stream_eof_settles_once_with_distinct_sample_count() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![stream_chunk(0), stream_chunk(0), stream_chunk(1)]);
    let observer = Arc::new(Observer::default());
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());
    let chunks: Vec<_> = client
        .generate_stream(stream_request())
        .await
        .unwrap()
        .collect()
        .await;
    assert_eq!(chunks.len(), 3);
    let events = observer.0.lock().unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Succeeded { image_count: 2 },
            ..
        }
    ));
    drop(events);
}

#[tokio::test]
async fn stream_explicit_cancel_is_settled_but_drop_is_not_guessed() {
    let observer = Arc::new(Observer::default());
    let client =
        Client::with_transport(MockTransport::default()).with_credit_observer(observer.clone());
    client
        .generate_stream(stream_request())
        .await
        .unwrap()
        .cancel()
        .await;
    {
        let events = observer.0.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(
            events[1],
            CreditEvent::RequestSettled {
                outcome: RequestOutcome::Cancelled,
                ..
            }
        ));
        drop(events);
    }

    let dropped = client.generate_stream(stream_request()).await.unwrap();
    drop(dropped);
    let events = observer.0.lock().unwrap();
    assert_eq!(events.len(), 3, "drop adds only the second planned event");
    assert!(matches!(events[2], CreditEvent::RequestPlanned { .. }));
    drop(events);
}

#[tokio::test]
async fn non_image_response_is_rejected() {
    let client =
        Client::with_transport(MockTransport::with_bytes_response(b"not an image".to_vec()));
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full).build();
    assert!(client.generate(request).await.is_err());
}

#[tokio::test]
async fn ordinary_and_stream_observation_share_effective_cost_fields() {
    for streaming in [false, true] {
        let transport = MockTransport::default();
        let observer = Arc::new(Observer::default());
        let client =
            Client::with_transport(transport.clone()).with_credit_observer(observer.clone());
        let mut request =
            GenerateImageRequest::builder("private prompt", Model::NaiDiffusion45Full)
                .size(ImageSize {
                    width: 2048,
                    height: 2048,
                })
                .steps(100)
                .n_samples(9)
                .build();
        request.guidance.smea = true;
        let plan = request.normalize().unwrap();
        let context = novelai_bridge::AnlasPricingContext::default();
        let cost = plan.anlas_estimate_input(context);
        let estimate = plan.estimate_anlas_cost(context);
        assert_eq!(
            client.estimate_anlas_cost(plan.request()).unwrap(),
            estimate
        );
        let expected_prompt_len = plan.resolved_prompts().prompt.chars().count();
        if streaming {
            let _stream = client
                .generate_stream_prepared(
                    plan.prepare().unwrap(),
                    novelai_bridge::StreamMode::default(),
                )
                .await
                .unwrap();
        } else {
            client
                .generate_prepared(plan.prepare().unwrap())
                .await
                .unwrap();
        }
        let body = transport.last_body().unwrap();
        assert_eq!(body["parameters"]["width"], cost.size.width);
        assert_eq!(body["parameters"]["height"], cost.size.height);
        assert_eq!(body["parameters"]["steps"], cost.steps);
        assert_eq!(body["parameters"]["n_samples"], cost.n_samples);
        assert!(!cost.smea);
        let events = observer.0.lock().unwrap();
        let (fingerprint, planned_cost) = events
            .iter()
            .find_map(|event| match event {
                CreditEvent::RequestPlanned {
                    fingerprint,
                    estimate,
                } => Some((fingerprint, estimate)),
                _ => None,
            })
            .unwrap();
        assert_eq!(fingerprint.size, Some(cost.size));
        assert_eq!(fingerprint.steps, Some(cost.steps));
        assert_eq!(fingerprint.n_samples, cost.n_samples);
        assert_eq!(fingerprint.smea, cost.smea);
        assert_eq!(
            usize::try_from(fingerprint.prompt_len_chars).unwrap(),
            expected_prompt_len
        );
        assert_eq!(
            *planned_cost,
            novelai_bridge::RequestCostEstimate::Generation(estimate)
        );
        drop(events);
    }
}
