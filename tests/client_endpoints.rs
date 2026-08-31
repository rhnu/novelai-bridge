mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use novelai_bridge::{
    ApiKeySource, BaseUrls, BridgeError, Client, ClientOptions, CreditEvent, CreditObserver,
    DirectorTool, EncodeVibeRequest, InvalidRequest, Model, RequestCostEstimate,
    RunDirectorToolRequest,
};

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<CreditEvent>>,
}

#[async_trait]
impl CreditObserver for RecordingObserver {
    async fn observe(&self, event: CreditEvent) {
        self.events.lock().unwrap().push(event);
    }
}
use serde_json::json;

use support::{
    MockTransport, create_png_base64, create_zip_with_single_png, create_zip_with_two_images,
};

#[tokio::test]
async fn encode_vibe_returns_base64() {
    let transport = MockTransport::default();
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::with_transport(transport.clone());

    let encoded = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            strict_mode: false,
        })
        .await
        .unwrap();

    assert_eq!(encoded, "AQID");
    let body = transport.last_body().expect("encode_vibe body");
    assert_eq!(body["image"], json!("QUJDRA=="));
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
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());

    client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            strict_mode: false,
        })
        .await
        .unwrap();

    let estimate = match &observer.events.lock().unwrap()[0] {
        CreditEvent::RequestPlanned { estimate, .. } => *estimate,
        event => panic!("expected planned event, got {event:?}"),
    };
    assert_eq!(estimate, RequestCostEstimate::Fixed { anlas: 2 });
}

#[tokio::test]
async fn encode_vibe_rejects_invalid_base64_without_transport_call() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,@@@".to_owned(),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            strict_mode: false,
        })
        .await
        .expect_err("invalid base64 should fail locally");
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidBase64 { ref field, .. })
            if field == "encode_vibe.image"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn encode_vibe_rejects_models_without_vibe_support() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 0.7,
            model: Model::NaiDiffusion5Full,
            strict_mode: false,
        })
        .await
        .expect_err("V5 has no vibe support");

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "encode_vibe"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn encode_vibe_clamps_information_extracted_when_not_strict() {
    let transport = MockTransport::default();
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::with_transport(transport.clone());

    let _ = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 2.0,
            model: Model::NaiDiffusion45Full,
            strict_mode: false,
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("encode_vibe body");
    assert_eq!(body["information_extracted"], json!(1.0));
}

#[tokio::test]
async fn encode_vibe_rejects_information_extracted_when_strict() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 2.0,
            model: Model::NaiDiffusion45Full,
            strict_mode: true,
        })
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if field == "information_extracted"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn director_tool_request_includes_req_type_dimensions_and_image() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());
    let image_base64 = create_png_base64(320, 240);

    let _ = client
        .run_director_tool(RunDirectorToolRequest {
            tool: DirectorTool::Lineart,
            image: format!("data:image/png;base64,{image_base64}"),
            prompt: None,
            defry: None,
            strict_mode: false,
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
async fn director_extracts_first_image_from_zip_and_fallbacks_to_raw_bytes() {
    let transport_zip = MockTransport::with_bytes_response(create_zip_with_two_images());
    let client_zip = Client::with_transport(transport_zip);
    let image = client_zip
        .run_director_tool(RunDirectorToolRequest {
            tool: DirectorTool::Sketch,
            image: create_png_base64(48, 48),
            prompt: None,
            defry: None,
            strict_mode: false,
        })
        .await
        .unwrap();
    assert_eq!(image.bytes, support::create_png_bytes(2, 2));

    let raw_png = support::create_png_bytes(2, 2);
    let transport_raw = MockTransport::with_bytes_response(raw_png.clone());
    let client_raw = Client::with_transport(transport_raw);
    let fallback = client_raw
        .run_director_tool(RunDirectorToolRequest {
            tool: DirectorTool::Sketch,
            image: create_png_base64(48, 48),
            prompt: None,
            defry: None,
            strict_mode: false,
        })
        .await
        .unwrap();
    assert_eq!(fallback.bytes, raw_png);
    assert_eq!(fallback.mime_type.as_deref(), Some("image/png"));
}

#[tokio::test]
async fn director_tool_clamps_defry_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let _ = client
        .run_director_tool(RunDirectorToolRequest {
            tool: DirectorTool::Colorize,
            image: create_png_base64(48, 48),
            prompt: None,
            defry: Some(9),
            strict_mode: false,
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("director body");
    assert_eq!(body["defry"], json!(5));
}

#[test]
fn client_options_debug_redacts_inline_api_key() {
    let options = ClientOptions {
        api_key_source: ApiKeySource::Inline {
            value: "secret-token".into(),
        },
        ..Default::default()
    };

    let debug = format!("{options:?}");
    assert!(!debug.contains("secret-token"));
    assert!(debug.contains("REDACTED"));
}

#[tokio::test]
async fn get_subscription_maps_balance_and_tier() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "active": true,
        "expiresAt": 1_712_345_678,
        "trainingStepsLeft": {
            "fixedTrainingStepsLeft": 100,
            "purchasedTrainingSteps": 23
        }
    }));
    let client = Client::with_transport(transport.clone());

    let subscription = client.get_subscription().await.unwrap();
    assert_eq!(subscription.anlas_balance, 123);
    assert!(subscription.is_opus);
    assert!(subscription.subscription_active);
    assert_eq!(subscription.tier_name, "opus");
    assert_eq!(subscription.v5_usage, None);
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/user/subscription")
    );
}

#[tokio::test]
async fn get_subscription_reads_v5_free_generation_allowance() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "trainingStepsLeft": { "fixedTrainingStepsLeft": 10 },
        "usage": {
            "isNegative": false,
            "percent": 87.6,
            "timeUntilNextPercent": 142
        }
    }));
    let client = Client::with_transport(transport);

    let subscription = client.get_subscription().await.unwrap();
    let usage = subscription.v5_usage.expect("v5 usage");
    assert!(!usage.is_negative);
    assert_eq!(usage.percent, 87);
    assert_eq!(usage.seconds_until_next_percent, 142);
}

/// Mirrors a live Opus response captured on 2026-08-27: integer `percent`, and a
/// `timeUntilNextPercent` that is a recharge rate rather than a countdown.
#[tokio::test]
async fn get_subscription_maps_a_live_opus_response() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "active": true,
        "accountType": 0,
        "isGracePeriod": false,
        "expiresAt": 1_790_345_896_i64,
        "perks": {
            "maxPriorityActions": 1000,
            "startPriority": 10,
            "unlimitedMaxPriority": true,
            "moduleTrainingSteps": 10_000
        },
        "trainingStepsLeft": {
            "fixedTrainingStepsLeft": 10_000,
            "purchasedTrainingSteps": 0
        },
        "usage": {
            "percent": 100,
            "isNegative": false,
            "timeUntilNextPercent": 7888
        }
    }));
    let client = Client::with_transport(transport);

    let subscription = client.get_subscription().await.unwrap();
    assert_eq!(subscription.anlas_balance, 10_000);
    assert!(subscription.is_opus);
    assert!(subscription.subscription_active);
    assert_eq!(subscription.expires_at_ms, Some(1_790_345_896_000));

    let usage = subscription.v5_usage.expect("v5 usage");
    assert!(!usage.is_negative);
    assert_eq!(usage.percent, 100);
    assert_eq!(usage.seconds_until_next_percent, 7888);
}

/// A missing recharge rate must not discard the whole allowance: that would also discard
/// `isNegative` and silently restore the Opus first-image allowance for V5.
#[tokio::test]
async fn get_subscription_keeps_the_allowance_without_a_recharge_rate() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "trainingStepsLeft": { "fixedTrainingStepsLeft": 10 },
        "usage": {
            "isNegative": true,
            "percent": 0
        }
    }));
    let client = Client::with_transport(transport);

    let subscription = client.get_subscription().await.unwrap();
    let usage = subscription.v5_usage.expect("v5 usage");
    assert!(usage.is_negative);
    assert_eq!(usage.percent, 0);
    assert_eq!(usage.seconds_until_next_percent, 0);
}

#[tokio::test]
async fn get_subscription_records_last_sampled_balance() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "trainingStepsLeft": { "purchasedTrainingSteps": 55 }
    }));
    let client = Client::with_transport(transport);
    assert!(client.last_sampled_balance().is_none());

    let _ = client.get_subscription().await.unwrap();
    let balance = client.last_sampled_balance().expect("sampled balance");
    assert_eq!(balance.anlas, 55);
    assert!(balance.is_opus);
}

#[tokio::test]
async fn get_subscription_infers_activity_from_expiration_and_honors_explicit_state() {
    let transport = MockTransport::default();
    transport.set_json_response(json!({
        "tier": 3,
        "expiresAt": 4_102_444_800_u64
    }));
    let client = Client::with_transport(transport.clone());
    assert!(client.get_subscription().await.unwrap().subscription_active);

    transport.set_json_response(json!({
        "tier": 3,
        "expiresAt": 4_102_444_800_u64,
        "active": false
    }));
    assert!(!client.get_subscription().await.unwrap().subscription_active);

    transport.set_json_response(json!({ "tier": 3, "expiresAt": 1 }));
    assert!(!client.get_subscription().await.unwrap().subscription_active);
}

#[tokio::test]
async fn custom_base_urls_route_image_service_endpoints() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    transport.set_json_response(json!({ "tier": 1 }));
    let client = Client::with_transport_and_base_urls(
        transport.clone(),
        BaseUrls {
            image: "https://image.example.test/root/".to_owned(),
            api: "https://api.example.test/root/".to_owned(),
        },
    );

    let _ = client
        .encode_vibe(EncodeVibeRequest {
            image: "data:image/png;base64,QUJDRA==".to_owned(),
            information_extracted: 0.7,
            model: Model::NaiDiffusion45Full,
            strict_mode: false,
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
