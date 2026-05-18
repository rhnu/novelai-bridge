mod support;

use novelai_bridge::{
    ApiKeySource, BaseUrls, BridgeError, Client, ClientOptions, DirectorTool, EncodeVibeRequest,
    InvalidRequest, Model, RunDirectorToolRequest,
};
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
    assert_eq!(image.bytes, vec![11_u8, 22, 33]);

    let raw_png = vec![0x89, b'P', b'N', b'G', 0_u8, 1, 2, 3];
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
    assert_eq!(subscription.tier_name, "opus");
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://api.novelai.net/user/subscription")
    );
}

#[tokio::test]
async fn custom_base_urls_route_image_and_api_endpoints() {
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
        Some("https://api.example.test/root/user/subscription")
    );
}
