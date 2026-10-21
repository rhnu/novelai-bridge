//! Contracts shared by generation preparation and client execution.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use novelai_bridge::{
    Character, CharacterPosition, Client, ClientConfig, CreditEvent, CreditObserver,
    GenerateImageRequest, ImageSize, Model,
};

use crate::support::MockTransport;

#[derive(Default)]
struct Observer(Mutex<Vec<CreditEvent>>);

#[async_trait]
impl CreditObserver for Observer {
    async fn observe(&self, event: CreditEvent) {
        self.0
            .lock()
            .expect("test event mutex is not poisoned")
            .push(event);
    }
}

#[tokio::test]
async fn public_estimate_matches_planned_estimate_after_size_normalization() {
    let transport = MockTransport::default();
    let observer = Arc::new(Observer::default());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default())
        .with_credit_observer(Arc::<Observer>::clone(&observer));
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
            CreditEvent::RequestPlanned { estimate, .. } => estimate.total(),
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
    Client::new(Arc::new(transport.clone()), ClientConfig::default())
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
    Client::new(Arc::new(transport.clone()), ClientConfig::default())
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
async fn non_image_response_is_rejected() {
    let client = Client::new(
        Arc::new(MockTransport::with_bytes_response(b"not an image".to_vec())),
        ClientConfig::default(),
    );
    let request = GenerateImageRequest::builder("test", Model::NaiDiffusion45Full).build();
    assert!(client.generate(request).await.is_err());
}

#[tokio::test]
async fn ordinary_and_stream_observation_share_effective_cost_fields() {
    for streaming in [false, true] {
        let transport = MockTransport::default();
        let observer = Arc::new(Observer::default());
        let client = Client::new(Arc::new(transport.clone()), ClientConfig::default())
            .with_credit_observer(Arc::<Observer>::clone(&observer));
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
        let (novelai_bridge::ObservedOperation::GenerateImage(summary)
        | novelai_bridge::ObservedOperation::GenerateImageStream(summary)) = &fingerprint.operation
        else {
            panic!("expected generation summary");
        };
        assert_eq!(summary.pricing.size, cost.size);
        assert_eq!(summary.pricing.steps, cost.steps);
        assert_eq!(summary.pricing.n_samples, cost.n_samples);
        assert_eq!(summary.pricing.smea, cost.smea);
        assert_eq!(
            usize::try_from(summary.prompt_len_chars).unwrap(),
            expected_prompt_len
        );
        assert_eq!(
            *planned_cost,
            novelai_bridge::RequestCostEstimate::Generation(estimate)
        );
        drop(events);
    }
}
