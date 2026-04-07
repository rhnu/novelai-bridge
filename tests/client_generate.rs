mod support;

use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use novelai_bridge::{
    Character, CharacterPosition, CharacterReference, CharacterReferenceType, Client,
    ControlNetConfig, ControlNetInput, GenerateImageRequest, GenerateImageStreamRequest,
    ImageFormat, Img2ImgRequest, Model,
};
use serde_json::json;

use support::{MockTransport, create_png_base64, create_zip_with_single_png};

#[tokio::test]
async fn generate_builds_baseline_text2img_request_shape() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        model: Model::NaiDiffusion3,
        seed: 42,
        ..Default::default()
    };
    let expected_prompt = params.processed_prompt();
    let expected_negative_prompt = params.processed_negative_prompt();

    let images = client.generate(params).await.unwrap();
    assert_eq!(images.len(), 1);

    let body = transport.last_body().expect("generate body");
    assert_eq!(transport.last_path().as_deref(), Some("/ai/generate-image"));
    assert_eq!(body["action"], json!("generate"));
    assert_eq!(body["input"], json!("1girl"));
    assert_eq!(body["model"], json!("nai-diffusion-3"));
    assert_eq!(body["parameters"]["prompt"], json!(expected_prompt));
    assert_eq!(
        body["parameters"]["negative_prompt"],
        json!(expected_negative_prompt)
    );
    assert_eq!(body["parameters"]["width"], json!(832));
    assert_eq!(body["parameters"]["height"], json!(1216));
    assert_eq!(body["parameters"]["seed"], json!(42));
    assert!(body["parameters"]["image"].is_null());
}

#[tokio::test]
async fn generate_serializes_image_format() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        model: Model::NaiDiffusion3,
        ..Default::default()
    };
    params.image_format = Some(ImageFormat::Webp);

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["image_format"], json!("webp"));
}

#[tokio::test]
async fn generate_with_auto_seed_reuses_resolved_seed_in_response() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let images = client
        .generate(GenerateImageRequest {
            prompt: "1girl".to_owned(),
            seed: 0,
            ..Default::default()
        })
        .await
        .unwrap();

    let body = transport.last_body().expect("generate body");
    let request_seed = body["parameters"]["seed"].as_i64().expect("request seed");
    assert_ne!(request_seed, 0);
    assert_eq!(images[0].seed, Some(request_seed));
}

#[tokio::test]
async fn generate_stream_parses_chunk() {
    let transport = MockTransport::default();
    transport.set_sse_lines(vec![
        "data: {\"event_type\":\"step\",\"samp_ix\":0,\"step_ix\":1,\"gen_id\":1,\"sigma\":1.1,\"image\":\"abc\"}".to_owned(),
    ]);
    let client = Client::with_transport(transport);

    let params = GenerateImageStreamRequest {
        base: GenerateImageRequest {
            prompt: "1girl".to_owned(),
            ..Default::default()
        },
        ..Default::default()
    };

    let mut stream = client.generate_stream(params).await.unwrap();
    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item.event_type, "step");
    assert_eq!(item.step_ix, Some(1));
}

#[tokio::test]
async fn generate_normalizes_i2i_image_and_mask_payloads() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.i2i = Some(Img2ImgRequest {
        image: "data:image/png;base64,QUJDRA==".to_owned(),
        strength: 0.7,
        noise: 0.2,
        mask: Some("data:image/png;base64,RUZHSA==".to_owned()),
    });

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["image"], json!("QUJDRA=="));
    assert_eq!(body["parameters"]["mask"], json!("RUZHSA=="));
}

#[tokio::test]
async fn generate_normalizes_character_reference_image_payloads() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.character_references = Some(vec![CharacterReference {
        image: format!("data:image/png;base64,{}", create_png_base64(800, 1200)),
        reference_type: CharacterReferenceType::CharacterAndStyle,
        fidelity: 0.5,
        strength: 0.6,
    }]);

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    let encoded = body["parameters"]["director_reference_images"][0]
        .as_str()
        .unwrap();
    let decoded = STANDARD.decode(encoded).unwrap();
    let image = image::load_from_memory(&decoded).unwrap();
    assert_eq!(image.width(), 1024);
    assert_eq!(image.height(), 1536);
}

#[tokio::test]
async fn generate_serializes_controlnet_and_reuses_vibe_cache() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::with_transport(transport.clone());

    let build_params = |image: &str| {
        let mut params = GenerateImageRequest {
            prompt: "1girl".to_owned(),
            ..Default::default()
        };
        params.controlnet = Some(ControlNetConfig {
            strength: 1.0,
            images: vec![ControlNetInput {
                image: image.to_owned(),
                info_extracted: 0.7,
                strength: 0.6,
                model: Model::NaiDiffusion45Full,
                vibe_data_cache: None,
            }],
        });
        params
    };

    let _ = client
        .generate(build_params("data:image/png;base64,QUJDRA=="))
        .await
        .unwrap();
    let _ = client.generate(build_params("QUJDRA==")).await.unwrap();

    let body = transport
        .bodies_for_path("/ai/generate-image")
        .pop()
        .expect("generate body");
    let paths = transport.byte_paths();
    let encode_calls = paths
        .iter()
        .filter(|path| path.as_str() == "/ai/encode-vibe")
        .count();
    let generate_calls = paths
        .iter()
        .filter(|path| path.as_str() == "/ai/generate-image")
        .count();
    assert_eq!(encode_calls, 1);
    assert_eq!(generate_calls, 2);
    assert_eq!(
        body["parameters"]["reference_image_multiple"][0],
        json!("AQID")
    );
    assert!(
        (body["parameters"]["reference_strength_multiple"][0]
            .as_f64()
            .unwrap()
            - 0.6)
            .abs()
            < 1e-6
    );
    assert!(
        (body["parameters"]["reference_information_extracted_multiple"][0]
            .as_f64()
            .unwrap()
            - 0.7)
            .abs()
            < 1e-6
    );
    assert!((body["parameters"]["controlnet_strength"].as_f64().unwrap() - 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn generate_rejects_invalid_vibe_data_cache_without_transport_call() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.controlnet = Some(ControlNetConfig {
        strength: 1.0,
        images: vec![ControlNetInput {
            image: String::new(),
            info_extracted: 0.7,
            strength: 0.6,
            model: Model::NaiDiffusion45Full,
            vibe_data_cache: Some("not-base64".to_owned()),
        }],
    });

    let error = client.generate(params).await.unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("vibe_data_cache"));
    assert!(transport.byte_paths().is_empty());
}

#[tokio::test]
async fn generate_rejects_base64_shaped_invalid_vibe_data_cache_without_transport_call() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.controlnet = Some(ControlNetConfig {
        strength: 1.0,
        images: vec![ControlNetInput {
            image: String::new(),
            info_extracted: 0.7,
            strength: 0.6,
            model: Model::NaiDiffusion45Full,
            vibe_data_cache: Some("AAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
        }],
    });

    let error = client.generate(params).await.unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("vibe_data_cache"));
    assert!(transport.byte_paths().is_empty());
}

#[tokio::test]
async fn character_positions_enable_use_coords_when_non_center_and_not_explicit() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.characters = Some(vec![
        Character {
            prompt: "char-a".to_owned(),
            negative_prompt: Some(String::new()),
            position: CharacterPosition { x: 0.11, y: 0.89 },
            enabled: true,
        },
        Character {
            prompt: "char-b".to_owned(),
            negative_prompt: Some(String::new()),
            position: CharacterPosition { x: 0.74, y: 0.24 },
            enabled: true,
        },
    ]);

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    let center = body["parameters"]["characterPrompts"][0]["center"]
        .as_object()
        .unwrap();

    assert_eq!(body["parameters"]["use_coords"], json!(true));
    assert_eq!(body["parameters"]["v4_prompt"]["use_coords"], json!(true));
    assert!((center["x"].as_f64().unwrap() - 0.1).abs() < 1e-6);
    assert!((center["y"].as_f64().unwrap() - 0.9).abs() < 1e-6);
}

#[tokio::test]
async fn explicit_use_coords_false_keeps_center_positions() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        use_coords: Some(false),
        ..Default::default()
    };
    params.characters = Some(vec![Character {
        prompt: "char-a".to_owned(),
        negative_prompt: Some(String::new()),
        position: CharacterPosition { x: 0.11, y: 0.89 },
        enabled: true,
    }]);

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    let center = body["parameters"]["characterPrompts"][0]["center"]
        .as_object()
        .unwrap();

    assert_eq!(body["parameters"]["use_coords"], json!(false));
    assert_eq!(body["parameters"]["v4_prompt"]["use_coords"], json!(false));
    assert!((center["x"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert!((center["y"].as_f64().unwrap() - 0.5).abs() < 1e-6);
}

#[tokio::test]
async fn validate_rejects_controlnet_and_character_references_together() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport);

    let mut params = GenerateImageRequest {
        prompt: "1girl".to_owned(),
        ..Default::default()
    };
    params.controlnet = Some(ControlNetConfig {
        strength: 1.0,
        images: vec![ControlNetInput {
            image: "data:image/png;base64,AAA".to_owned(),
            info_extracted: 1.0,
            strength: 1.0,
            model: Model::NaiDiffusion45Full,
            vibe_data_cache: None,
        }],
    });
    params.character_references = Some(vec![CharacterReference {
        image: "data:image/png;base64,BBB".to_owned(),
        reference_type: CharacterReferenceType::Character,
        fidelity: 0.5,
        strength: 0.6,
    }]);

    let error = client.generate(params).await.unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("cannot be used together"));
}

#[tokio::test]
async fn generate_rejects_empty_prompt_locally() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client
        .generate(GenerateImageRequest {
            prompt: "   ".to_owned(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("prompt cannot be empty"));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_rejects_invalid_scale_locally() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client
        .generate(GenerateImageRequest {
            prompt: "1girl".to_owned(),
            scale: 11.0,
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("scale must be between 0 and 10"));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_does_not_parse_prompt_syntax_locally() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest {
        prompt: "{bad prompt syntax".to_owned(),
        ..Default::default()
    };

    let images = client.generate(params).await.unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(transport.last_path().as_deref(), Some("/ai/generate-image"));
}
