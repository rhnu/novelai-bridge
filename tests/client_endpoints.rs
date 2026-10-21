use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use novelai_bridge::{
    BridgeError, Client, ClientConfig, CreditEvent, CreditObserver, DirectorCommand,
    EncodeVibeRequest, EncodedImage, InvalidRequest, Model, RequestCostEstimate,
    RunDirectorToolRequest,
};

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<CreditEvent>>,
}

#[async_trait]
impl CreditObserver for RecordingObserver {
    async fn observe(&self, event: CreditEvent) {
        self.events
            .lock()
            .expect("test event mutex is not poisoned")
            .push(event);
    }
}

use serde_json::json;

use crate::support::{
    MockTransport, create_png_base64, create_zip_with_single_png, create_zip_with_two_images,
};

fn image(width: u32, height: u32) -> EncodedImage {
    EncodedImage::from_base64(&create_png_base64(width, height)).expect("fixture image is valid")
}

#[tokio::test]
async fn encode_vibe_returns_base64() {
    let transport = MockTransport::default();
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let encoded = client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();

    assert_eq!(encoded.to_base64(), "AQID");
    let body = transport.last_body().expect("encode_vibe body");
    assert_eq!(body["image"], json!(create_png_base64(2, 2)));
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/encode-vibe")
    );
}

#[tokio::test]
async fn encode_vibe_observation_reports_a_fixed_cost() {
    let transport = MockTransport::default();
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), ClientConfig::default())
        .with_credit_observer(Arc::<RecordingObserver>::clone(&observer));

    client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();

    let estimate = match &observer.events.lock().unwrap()[0] {
        CreditEvent::RequestPlanned { estimate, .. } => *estimate,
        event => panic!("expected planned event, got {event:?}"),
    };
    assert_eq!(estimate, RequestCostEstimate::Fixed { anlas: 2 });
}

#[test]
fn encoded_image_rejects_invalid_base64_at_construction() {
    let error = EncodedImage::from_base64("data:image/png;base64,@@@").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidBase64 { ref field, .. })
            if *field == novelai_bridge::RequestField::Image
    ));
}

#[tokio::test]
async fn encode_vibe_rejects_models_without_vibe_support() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let error = client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 0.7,
            model: Model::NaiDiffusion5Full,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .expect_err("V5 has no vibe support");

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if *feature == novelai_bridge::RequestField::EncodeVibe
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn encode_vibe_clamps_information_extracted_when_not_strict() {
    let transport = MockTransport::default();
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let _ = client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 2.0,
            model: Model::NaiDiffusion45Full,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("encode_vibe body");
    assert_eq!(body["information_extracted"], json!(1.0));
}

#[tokio::test]
async fn encode_vibe_rejects_information_extracted_when_strict() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let error = client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 2.0,
            model: Model::NaiDiffusion45Full,
            policy: novelai_bridge::ValidationPolicy::Reject,
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if *field == novelai_bridge::RequestField::VibeInformationExtracted
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn director_tool_request_includes_req_type_dimensions_and_image() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
    let image_base64 = create_png_base64(320, 240);

    let _ = client
        .run_director_tool(RunDirectorToolRequest {
            image: EncodedImage::from_base64(&image_base64).unwrap(),
            command: DirectorCommand::Lineart,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("director body");
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/augment-image")
    );
    assert_eq!(body["req_type"], json!("lineart"));
    assert_eq!(body["width"], json!(320));
    assert_eq!(body["height"], json!(240));
    assert_eq!(body["image"], json!(image_base64));
}

#[tokio::test]
async fn director_preserves_all_zip_images_and_accepts_raw_images() {
    let transport_zip = MockTransport::with_bytes_response(create_zip_with_two_images());
    let client_zip = Client::new(Arc::new(transport_zip), ClientConfig::default());
    let generated = client_zip
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Sketch,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();
    assert_eq!(generated.images.len(), 2);
    assert_eq!(
        generated.images[0].bytes(),
        crate::support::create_png_bytes(2, 2)
    );

    let raw_png = crate::support::create_png_bytes(2, 2);
    let transport_raw = MockTransport::with_bytes_response(raw_png.clone());
    let client_raw = Client::new(Arc::new(transport_raw), ClientConfig::default());
    let fallback = client_raw
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Sketch,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();
    assert_eq!(fallback.images.len(), 1);
    assert_eq!(fallback.images[0].bytes(), raw_png);
    assert_eq!(fallback.images[0].mime_type(), "image/png");
}

#[tokio::test]
async fn director_tool_clamps_defry_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let _ = client
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Colorize {
                prompt: None,
                defry: 9,
            },
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("director body");
    assert_eq!(body["defry"], json!(5));
}

#[tokio::test]
async fn director_emotion_requires_nonempty_prompt_and_preserves_valid_parameters() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
    let error = client
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Emotion {
                prompt: "  ".to_owned(),
                defry: 0,
            },
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::RequiredFieldForContext { ref context, .. })
            if context == "emotion"
    ));
    assert!(transport.last_path().is_none());

    client
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Emotion {
                prompt: " happy ".to_owned(),
                defry: 3,
            },
            policy: novelai_bridge::ValidationPolicy::Reject,
        })
        .await
        .unwrap();
    let body = transport.last_body().unwrap();
    assert_eq!(body["req_type"], json!("emotion"));
    assert_eq!(body["prompt"], json!("happy"));
    assert_eq!(body["defry"], json!(3));
}

#[tokio::test]
async fn director_rejects_out_of_range_defry_in_strict_mode() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
    let error = client
        .run_director_tool(RunDirectorToolRequest {
            image: image(48, 48),
            command: DirectorCommand::Colorize {
                prompt: None,
                defry: 6,
            },
            policy: novelai_bridge::ValidationPolicy::Reject,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if *field == novelai_bridge::RequestField::DirectorDefry
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn get_subscription_records_last_subscription() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "trainingStepsLeft": { "purchasedTrainingSteps": 55 }
    }));
    let client = Client::new(Arc::new(transport), ClientConfig::default());
    assert!(client.last_subscription().is_none());

    let _ = client.get_subscription().await.unwrap();
    let balance = client.last_subscription().expect("sampled balance");
    assert_eq!(balance.anlas_balance, 55);
    assert!(balance.is_opus());
}

#[tokio::test]
async fn custom_base_urls_route_image_service_endpoints() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    transport.set_encode_vibe_response(vec![1]);
    transport.set_json_response(
        json!({ "tier": 1, "trainingStepsLeft": { "fixedTrainingStepsLeft": 0 } }),
    );
    let client = Client::new(
        Arc::new(transport.clone()),
        ClientConfig {
            image_base_url: "https://image.example.test/root/".to_owned(),
            ..Default::default()
        },
    );

    let _ = client
        .encode_vibe(EncodeVibeRequest {
            image: image(2, 2),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            policy: novelai_bridge::ValidationPolicy::Normalize,
        })
        .await
        .unwrap();
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.example.test/root/ai/encode-vibe")
    );

    let _ = client.get_subscription().await.unwrap();
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.example.test/root/user/subscription")
    );
}
