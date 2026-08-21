mod support;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use novelai_bridge::{
    BaseUrls, BridgeError, Character, CharacterPosition, CharacterReference,
    CharacterReferenceType, CharacterSet, Client, CreditEvent, CreditObserver, GenerateImageRequest,
    GenerateImageStreamRequest, GeneratedImageMetadata, GuidanceParams, ImageFormat, ImageSize,
    ImageStreamChunk, Img2ImgRequest, InvalidRequest, Model, ObservedOperation, QualityPreset,
    ReferenceSet,
    RequestOutcome, UcPreset, VibeReference, VibeTransferConfig,
};
use serde_json::{Value, json};

use support::{
    MockTransport, create_png_base64, create_png_with_comment, create_zip_with_pngs,
    create_zip_with_single_png,
};

fn request(prompt: &str) -> GenerateImageRequest {
    GenerateImageRequest::builder(prompt, Model::default()).build()
}

fn request_for(prompt: &str, model: Model) -> GenerateImageRequest {
    GenerateImageRequest::builder(prompt, model).build()
}

fn png_data_url(width: u32, height: u32) -> String {
    format!("data:image/png;base64,{}", create_png_base64(width, height))
}

fn decoded_dimensions(value: &Value) -> (u32, u32) {
    let encoded = value.as_str().expect("base64 image payload");
    let bytes = STANDARD.decode(encoded).expect("decode base64");
    let image = image::load_from_memory(&bytes).expect("decode image");
    (image.width(), image.height())
}

#[tokio::test]
async fn generate_builds_baseline_text2img_request_shape() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = request_for("1girl", Model::NaiDiffusion3);
    params.core.seed = 42;
    let expected_prompt = params.processed_prompt();
    let expected_negative_prompt = params.processed_negative_prompt();

    let result = client.generate(params).await.unwrap();
    assert_eq!(result.images.len(), 1);

    let body = transport.last_body().expect("generate body");
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/generate-image")
    );
    assert_eq!(body["action"], json!("generate"));
    assert_eq!(body["input"], json!("1girl"));
    assert_eq!(body["model"], json!("nai-diffusion-3"));
    assert_eq!(body["parameters"]["prompt"], json!(expected_prompt));
    assert_eq!(
        body["parameters"]["negative_prompt"],
        json!(expected_negative_prompt)
    );
    assert_eq!(body["parameters"]["uc"], json!(expected_negative_prompt));
    assert_eq!(body["parameters"]["width"], json!(832));
    assert_eq!(body["parameters"]["height"], json!(1216));
    assert_eq!(body["parameters"]["seed"], json!(42));
    assert_eq!(body["parameters"]["params_version"], json!(3));
    assert!(body["parameters"]["image"].is_null());
    assert!(body["parameters"]["v4_prompt"].is_null());
}

#[tokio::test]
async fn generate_builds_v5_request_shape() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = request_for("1girl", Model::NaiDiffusion5Full);
    let _ = client.generate(params).await.unwrap();

    let body = transport.last_body().expect("generate body");
    assert_eq!(body["model"], json!("nai-diffusion-5-full"));
    assert_eq!(body["parameters"]["params_version"], json!(4));
    assert_eq!(
        body["parameters"]["tag_hint_transparent_background"],
        json!(true)
    );
    assert_eq!(body["parameters"]["legacy_uc"], json!(false));
    assert!(body["parameters"]["v4_prompt"].is_object());
    // V5 has no legacy SMEA or decrisp parameters.
    assert!(body["parameters"]["sm"].is_null());
    assert!(body["parameters"]["sm_dyn"].is_null());
    assert!(body["parameters"]["dynamic_thresholding"].is_null());
}

#[tokio::test]
async fn generate_omits_v5_extensions_for_v45() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let _ = client
        .generate(request_for("1girl", Model::NaiDiffusion45Full))
        .await
        .unwrap();

    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["params_version"], json!(3));
    assert!(body["parameters"]["tag_hint_transparent_background"].is_null());
    assert!(body["parameters"]["legacy_uc"].is_null());
}

#[tokio::test]
async fn generate_serializes_image_format() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .image_format(ImageFormat::Webp)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["image_format"], json!("webp"));
}

#[tokio::test]
async fn variety_boost_sigma_scales_with_model_and_canvas() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let sigma_for = |model: Model, size: ImageSize| {
        let transport = transport.clone();
        let client = client.clone();
        async move {
            let params = GenerateImageRequest::builder("1girl", model)
                .size(size)
                .variety_boost(true)
                .build();
            let _ = client.generate(params).await.unwrap();
            transport.last_body().expect("generate body")["parameters"]["skip_cfg_above_sigma"]
                .as_f64()
                .expect("sigma")
        }
    };

    // 832x1216 is the reference canvas, so the raw coefficient is sent unscaled.
    let v45_portrait = sigma_for(Model::NaiDiffusion45Full, ImageSize::portrait()).await;
    assert!((v45_portrait - 58.0).abs() < 1e-3, "{v45_portrait}");

    let plain_v4_portrait = sigma_for(Model::NaiDiffusion4Full, ImageSize::portrait()).await;
    assert!(
        (plain_v4_portrait - 19.0).abs() < 1e-3,
        "{plain_v4_portrait}"
    );

    // Golden value captured from the NovelAI web client at 1024x1024 on V4.5.
    let v45_square = sigma_for(Model::NaiDiffusion45Full, ImageSize::square()).await;
    assert!(
        (v45_square - 59.047_226_004_152_17).abs() < 1e-3,
        "{v45_square}"
    );
}

#[tokio::test]
async fn variety_boost_is_dropped_for_v5() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .variety_boost(true)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["skip_cfg_above_sigma"].is_null());
}

#[tokio::test]
async fn variety_boost_is_rejected_for_v5_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .variety_boost(true)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "guidance.variety_boost"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn img2img_request_uses_img2img_action_and_flat_fields() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut img2img = Img2ImgRequest::new(png_data_url(400, 600), 0.7, 0.2);
    img2img.extra_noise_seed = Some(999);
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .seed(42)
        .img2img(img2img)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");

    assert_eq!(body["action"], json!("img2img"));
    assert_eq!(body["model"], json!("nai-diffusion-4-5-full"));
    assert!((body["parameters"]["strength"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    assert!((body["parameters"]["noise"].as_f64().unwrap() - 0.2).abs() < 1e-6);
    assert_eq!(body["parameters"]["extra_noise_seed"], json!(999));
    assert_eq!(body["parameters"]["color_correct"], json!(true));
    assert!(body["parameters"]["mask"].is_null());
    assert!(body["parameters"]["request_type"].is_null());
}

#[tokio::test]
async fn img2img_defaults_extra_noise_seed_to_resolved_seed() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .seed(1234)
        .img2img(Img2ImgRequest::new(png_data_url(64, 64), 0.7, 0.0))
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["extra_noise_seed"], json!(1234));
}

#[tokio::test]
async fn masked_img2img_request_uses_infill_action_and_inpainting_model() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut img2img = Img2ImgRequest::new(png_data_url(400, 600), 0.6, 0.1);
    img2img.mask = Some(png_data_url(400, 600));
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .img2img(img2img)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");

    assert_eq!(body["action"], json!("infill"));
    assert_eq!(body["model"], json!("nai-diffusion-4-5-full-inpainting"));
    assert_eq!(
        body["parameters"]["request_type"],
        json!("NativeInfillingRequest")
    );
    assert!(body["parameters"]["mask"].is_string());
    // Infill carries the user strength through inpaintImg2ImgStrength, not `strength`.
    assert!(body["parameters"]["strength"].is_null());
    assert!(
        (body["parameters"]["inpaintImg2ImgStrength"]
            .as_f64()
            .unwrap()
            - 0.6)
            .abs()
            < 1e-6
    );
}

#[tokio::test]
async fn infill_uses_inpainting_model_for_each_family() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    for (model, expected) in [
        (Model::NaiDiffusion5Full, "nai-diffusion-5-full-inpainting"),
        (Model::NaiDiffusion4Curated, "nai-diffusion-4-curated-inpainting"),
        (Model::NaiDiffusion3, "nai-diffusion-3-inpainting"),
        (Model::NaiDiffusion3Furry, "nai-diffusion-furry-3-inpainting"),
    ] {
        let mut img2img = Img2ImgRequest::new(png_data_url(128, 128), 0.6, 0.0);
        img2img.mask = Some(png_data_url(128, 128));
        let params = GenerateImageRequest::builder("1girl", model)
            .img2img(img2img)
            .build();

        let _ = client.generate(params).await.unwrap();
        let body = transport.last_body().expect("generate body");
        assert_eq!(body["model"], json!(expected));
    }
}

#[tokio::test]
async fn img2img_source_and_mask_are_fitted_to_the_requested_canvas() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut img2img = Img2ImgRequest::new(png_data_url(300, 100), 0.7, 0.0);
    img2img.mask = Some(png_data_url(50, 50));
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .size(ImageSize::square())
        .img2img(img2img)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");

    assert_eq!(decoded_dimensions(&body["parameters"]["image"]), (1024, 1024));
    assert_eq!(decoded_dimensions(&body["parameters"]["mask"]), (1024, 1024));
}

#[tokio::test]
async fn generate_rejects_undecodable_img2img_source() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .img2img(Img2ImgRequest::new(
            "data:image/png;base64,QUJDRA==",
            0.7,
            0.0,
        ))
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UndecodableImage { ref field, .. })
            if field == "img2img.image"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_with_auto_seed_returns_resolved_request_seed_without_faking_image_seed() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let result = client.generate(request("1girl")).await.unwrap();

    let body = transport.last_body().expect("generate body");
    let request_seed = body["parameters"]["seed"].as_i64().expect("request seed");
    assert_ne!(request_seed, 0);
    assert_eq!(result.resolved_seed, request_seed);
    assert_eq!(result.images[0].seed(), None);
}

#[tokio::test]
async fn generate_with_fixed_seed_returns_exact_request_seed() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport);

    let mut params = request("1girl");
    params.core.seed = 4242;
    let result = client.generate(params).await.unwrap();

    assert_eq!(result.resolved_seed, 4242);
    assert_eq!(result.images[0].seed(), None);
}

#[tokio::test]
async fn generate_parses_each_zip_images_embedded_seed() {
    let first = create_png_with_comment(&json!({"prompt": "first", "seed": 101}));
    let second = create_png_with_comment(&json!({"prompt": "second", "seed": 202}));
    let transport = MockTransport::with_bytes_response(create_zip_with_pngs(&[
        ("first.png", first),
        ("second.png", second),
    ]));
    let client = Client::with_transport(transport);

    let mut params = request("request prompt");
    params.core.seed = 77;
    let result = client.generate(params).await.unwrap();

    assert_eq!(result.resolved_seed, 77);
    assert_eq!(result.images[0].seed(), Some(101));
    assert_eq!(result.images[1].seed(), Some(202));
}

#[tokio::test]
async fn generate_keeps_images_when_metadata_is_missing_or_invalid() {
    let valid_without_metadata = STANDARD
        .decode(create_png_base64(2, 2))
        .expect("decode png");
    let invalid_png = vec![0x89, b'P', b'N', b'G', 0, 1, 2, 3];
    let transport = MockTransport::with_bytes_response(create_zip_with_pngs(&[
        ("missing.png", valid_without_metadata),
        ("invalid.png", invalid_png),
    ]));
    let client = Client::with_transport(transport);

    let result = client.generate(request("1girl")).await.unwrap();

    assert_eq!(result.images.len(), 2);
    assert!(matches!(
        result.images[0].metadata,
        GeneratedImageMetadata::NotPresent
    ));
    assert!(matches!(
        result.images[1].metadata,
        GeneratedImageMetadata::Invalid { .. }
    ));
}

#[tokio::test]
async fn generate_stream_parses_chunk() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![ImageStreamChunk {
        event_type: "step".to_owned(),
        samp_ix: 0,
        step_ix: Some(1),
        gen_id: 1,
        sigma: Some(1.1),
        image: "abc".to_owned(),
    }]);
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageStreamRequest {
        base: request("1girl"),
        ..Default::default()
    };

    let mut stream = client.generate_stream(params).await.unwrap();
    assert_ne!(stream.resolved_seed, 0);
    assert_eq!(
        stream.resolved_seed,
        transport.last_body().unwrap()["parameters"]["seed"]
            .as_i64()
            .unwrap()
    );
    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item.event_type, "step");
    assert_eq!(item.step_ix, Some(1));
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/generate-image-stream")
    );
}

#[tokio::test]
async fn generate_stream_uses_custom_image_base_url() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![]);
    let client = Client::with_transport_and_base_urls(
        transport.clone(),
        BaseUrls {
            image: "https://image.example.test/root/".to_owned(),
            api: "https://api.example.test/root/".to_owned(),
        },
    );

    let params = GenerateImageStreamRequest {
        base: request("1girl"),
        ..Default::default()
    };

    let _stream = client.generate_stream(params).await.unwrap();
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.example.test/root/ai/generate-image-stream")
    );
}

#[tokio::test]
async fn generate_normalizes_character_reference_image_payloads() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .character_references(vec![CharacterReference {
            image: png_data_url(800, 1200),
            reference_type: CharacterReferenceType::CharacterAndStyle,
            fidelity: 0.5,
            strength: 0.6,
        }])
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(
        decoded_dimensions(&body["parameters"]["director_reference_images"][0]),
        (1024, 1536)
    );
}

#[tokio::test]
async fn generate_serializes_character_reference_director_fields() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .character_references(vec![
            CharacterReference {
                image: png_data_url(800, 1200),
                reference_type: CharacterReferenceType::Character,
                fidelity: 0.25,
                strength: 0.8,
            },
            CharacterReference {
                image: png_data_url(1200, 800),
                reference_type: CharacterReferenceType::Style,
                fidelity: 0.7,
                strength: 0.3,
            },
        ])
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    let parameters = &body["parameters"];

    assert_eq!(
        parameters["director_reference_descriptions"][0]["caption"]["base_caption"],
        json!("character")
    );
    assert_eq!(
        parameters["director_reference_descriptions"][1]["caption"]["base_caption"],
        json!("style")
    );
    assert_eq!(
        parameters["director_reference_information_extracted"],
        json!([1.0, 1.0])
    );
    let secondary = parameters["director_reference_secondary_strength_values"]
        .as_array()
        .unwrap();
    let strengths = parameters["director_reference_strength_values"]
        .as_array()
        .unwrap();
    assert!((secondary[0].as_f64().unwrap() - 0.75).abs() < 1e-6);
    assert!((secondary[1].as_f64().unwrap() - 0.3).abs() < 1e-6);
    assert!((strengths[0].as_f64().unwrap() - 0.8).abs() < 1e-6);
    assert!((strengths[1].as_f64().unwrap() - 0.3).abs() < 1e-6);
}

#[tokio::test]
async fn generate_rejects_character_references_on_v4_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion4Full)
        .character_references(vec![CharacterReference {
            image: png_data_url(64, 64),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.6,
        }])
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "character_references"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_drops_character_references_on_v4_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion4Full)
        .character_references(vec![CharacterReference {
            image: png_data_url(64, 64),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.6,
        }])
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["director_reference_images"].is_null());
}

#[tokio::test]
async fn generate_drops_references_on_v5_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .character_references(vec![CharacterReference {
            image: png_data_url(64, 64),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.6,
        }])
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["director_reference_images"].is_null());
}

#[tokio::test]
async fn generate_rejects_vibe_transfer_on_v5_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .vibe_transfer(VibeTransferConfig {
            references: vec![VibeReference {
                vibe_data_cache: "QUJDRA==".to_owned(),
                strength: 0.6,
            }],
            strength: 0.8,
        })
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "vibe_transfer"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_serializes_explicit_vibe_data_cache() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 0.6,
                vibe_data_cache: "AQID".to_owned(),
            }],
        })
        .build();

    let _ = client.generate(params).await.unwrap();

    let body = transport
        .bodies_for_path("https://image.novelai.net/ai/generate-image")
        .pop()
        .expect("generate body");
    let paths = transport.byte_paths();
    let encode_calls = paths
        .iter()
        .filter(|path| path.as_str() == "https://image.novelai.net/ai/encode-vibe")
        .count();
    assert_eq!(encode_calls, 0);
    assert_eq!(
        body["parameters"]["reference_image_multiple"][0],
        json!("AQID")
    );
    assert_eq!(
        body["parameters"]["normalize_reference_strength_multiple"],
        json!(true)
    );
    assert!(
        (body["parameters"]["reference_strength_multiple"][0]
            .as_f64()
            .unwrap()
            - 0.6)
            .abs()
            < 1e-6
    );
    // A live A/B showed this field changes nothing and is accepted when absent, so it is no
    // longer sent: encode_vibe bakes the value into the cache.
    assert!(body["parameters"]["reference_information_extracted_multiple"].is_null());
    assert!((body["parameters"]["controlnet_strength"].as_f64().unwrap() - 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn generate_omits_vibe_normalization_flag_without_references() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let _ = client.generate(request("1girl")).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["normalize_reference_strength_multiple"].is_null());
    assert!(body["parameters"]["reference_image_multiple"].is_null());
}

#[tokio::test]
async fn generate_rejects_vibe_transfer_without_explicit_vibe_data_cache() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 0.6,
                vibe_data_cache: String::new(),
            }],
        })
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if field == "vibe_transfer.references[0].vibe_data_cache"
    ));
    assert!(transport.byte_paths().is_empty());
}

#[tokio::test]
async fn generate_rejects_invalid_vibe_data_cache_without_transport_call() {
    for cache in ["not-base64", "AAAAAAAAAAAAAAAAAAAAAAAAA"] {
        let transport = MockTransport::default();
        let client = Client::with_transport(transport.clone());

        let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
            .vibe_transfer(VibeTransferConfig {
                strength: 1.0,
                references: vec![VibeReference {
                    strength: 0.6,
                    vibe_data_cache: cache.to_owned(),
                }],
            })
            .build();

        let error = client.generate(params).await.unwrap_err();
        assert!(matches!(
            error,
            BridgeError::InvalidRequest(InvalidRequest::InvalidBase64 { ref field, .. })
                if field == "vibe_data_cache"
        ));
        assert!(transport.byte_paths().is_empty());
    }
}

#[tokio::test]
async fn character_positions_enable_use_coords_when_non_center_and_not_explicit() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .characters(vec![
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
        ])
        .build();

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

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .characters(vec![Character {
            prompt: "char-a".to_owned(),
            negative_prompt: Some(String::new()),
            position: CharacterPosition { x: 0.11, y: 0.89 },
            enabled: true,
        }])
        .use_coords(false)
        .build();

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
async fn characters_are_rejected_on_v3_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .characters(vec![Character {
            prompt: "char-a".to_owned(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: true,
        }])
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "characters"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn validate_rejects_vibe_transfer_and_character_references_together() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport);

    let mut params = request_for("1girl", Model::NaiDiffusion45Full);
    params.strict_mode = true;
    params.references = Some(ReferenceSet {
        vibe_transfer: Some(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 1.0,
                vibe_data_cache: "AQID".to_owned(),
            }],
        }),
        character_references: vec![CharacterReference {
            image: png_data_url(64, 64),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.6,
        }],
    });

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedFieldCombination {
            ref left,
            ref right,
        }) if left == "vibe_transfer" && right == "character_references"
    ));
}

#[tokio::test]
async fn infill_drops_vibe_transfer_to_avoid_server_error() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut img2img = Img2ImgRequest::new(png_data_url(128, 128), 0.6, 0.0);
    img2img.mask = Some(png_data_url(128, 128));
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .img2img(img2img)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 1.0,
                vibe_data_cache: "AQID".to_owned(),
            }],
        })
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["action"], json!("infill"));
    assert!(body["parameters"]["reference_image_multiple"].is_null());
}

#[tokio::test]
async fn generate_rejects_empty_prompt_locally() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let error = client.generate(request("   ")).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if field == "prompt"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_rejects_invalid_scale_locally() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::default())
        .scale(11.0)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if field == "scale"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_clamps_invalid_numeric_values_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let mut params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .scale(11.0)
        .size(ImageSize {
            width: 17,
            height: 1599,
        })
        .img2img(Img2ImgRequest::new(png_data_url(64, 64), 5.0, -0.5))
        .build();
    params.sampling.cfg_rescale = -0.5;
    params.core.n_samples = 9;

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["scale"], json!(10.0));
    assert_eq!(body["parameters"]["cfg_rescale"], json!(0.0));
    assert_eq!(body["parameters"]["n_samples"], json!(4));
    assert_eq!(body["parameters"]["width"], json!(64));
    assert_eq!(body["parameters"]["height"], json!(1600));
    assert!((body["parameters"]["strength"].as_f64().unwrap() - 0.99).abs() < 1e-6);
    assert_eq!(body["parameters"]["noise"], json!(0.0));
}

#[tokio::test]
async fn generate_shrinks_canvas_over_the_pixel_budget_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 1600,
            height: 1600,
        })
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    let width = body["parameters"]["width"].as_u64().unwrap();
    let height = body["parameters"]["height"].as_u64().unwrap();
    assert!(width * height <= 3_145_728, "{width}x{height}");
    assert_eq!(width % 64, 0);
    assert_eq!(height % 64, 0);
}

#[tokio::test]
async fn generate_clamps_reference_numeric_values_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 2.0,
            references: vec![VibeReference {
                strength: -1.0,
                vibe_data_cache: "AQID".to_owned(),
            }],
        })
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["controlnet_strength"], json!(1.0));
    assert_eq!(
        body["parameters"]["reference_strength_multiple"][0],
        json!(0.0)
    );
}

#[tokio::test]
async fn generate_rejects_reference_numeric_values_when_strict() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 5.0,
                vibe_data_cache: "AQID".to_owned(),
            }],
        })
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if field == "vibe_transfer.references[0].strength"
    ));
    assert!(transport.last_path().is_none());
}

#[test]
fn uc_preset_uses_light_default_and_rejects_strong_alias() {
    assert_eq!(UcPreset::default(), UcPreset::Light);
    assert!(serde_json::from_str::<UcPreset>("\"strong\"").is_err());
    assert_eq!(
        serde_json::from_str::<UcPreset>("\"heavy\"").unwrap(),
        UcPreset::Heavy
    );
}

#[test]
fn uc_preset_text_is_merged_before_the_caller_negative_prompt() {
    let mut params = request_for("1girl", Model::NaiDiffusion45Full);
    params.core.negative_prompt = Some("extra tags".to_owned());
    params.core.uc_preset = UcPreset::Heavy;

    let negative = params.processed_negative_prompt();
    assert!(negative.starts_with("nsfw, lowres,"), "{negative}");
    assert!(negative.ends_with(", extra tags"), "{negative}");
}

#[test]
fn uc_preset_none_keeps_the_caller_negative_prompt_verbatim() {
    let mut params = request("1girl");
    params.core.negative_prompt = Some("only mine".to_owned());
    params.core.uc_preset = UcPreset::None;
    assert_eq!(params.processed_negative_prompt(), "only mine");
}

#[test]
fn quality_presets_swap_the_appended_tags() {
    let standard = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .quality(QualityPreset::Standard)
        .build();
    let light = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .quality(QualityPreset::Light)
        .build();
    let none = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .quality(QualityPreset::None)
        .build();

    assert_eq!(
        standard.processed_prompt(),
        "1girl, very aesthetic, masterpiece, no text"
    );
    // Light differs from Standard only in swapping `masterpiece` for `amazing quality`.
    assert_eq!(
        light.processed_prompt(),
        "1girl, very aesthetic, amazing quality, no text"
    );
    assert_eq!(none.processed_prompt(), "1girl");

    // Standard is what the pre-V5 quality toggle always produced.
    assert_eq!(
        GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
            .build()
            .processed_prompt(),
        standard.processed_prompt()
    );
}

#[tokio::test]
async fn quality_none_still_reports_the_toggle_state() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .quality(QualityPreset::None)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["qualityToggle"], json!(false));
    assert_eq!(
        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
        json!("1girl")
    );
}

#[tokio::test]
async fn light_quality_preset_falls_back_to_standard_off_v5() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .quality(QualityPreset::Light)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(
        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
        json!("1girl, very aesthetic, masterpiece, no text")
    );
}

#[tokio::test]
async fn light_quality_preset_is_rejected_off_v5_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .quality(QualityPreset::Light)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "quality.light"
    ));
    assert!(transport.last_path().is_none());
}

#[test]
fn sampling_defaults_follow_model_capabilities() {
    let v5 = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full).build();
    let v45 = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full).build();
    let v4 = GenerateImageRequest::builder("1girl", Model::NaiDiffusion4Full).build();
    let furry = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3Furry).build();

    assert!((v5.sampling.scale - 7.0).abs() < 1e-6);
    assert!((v45.sampling.scale - 5.0).abs() < 1e-6);
    assert!((v4.sampling.scale - 5.5).abs() < 1e-6);
    assert!((furry.sampling.scale - 6.2).abs() < 1e-6);
    assert_eq!(v5.sampling.steps, 23);
}

#[test]
fn capabilities_describe_each_model_family() {
    let v5 = Model::NaiDiffusion5Full.capabilities();
    assert!(v5.uses_v5_extensions);
    assert_eq!(v5.params_version, 4);
    // V5 drops variety boost, vibe transfer and character reference entirely.
    assert!(!v5.supports_variety_boost);
    assert!(!v5.supports_vibe_transfer);
    assert!(!v5.supports_encoded_vibe);
    assert!(!v5.supports_character_reference);
    assert_eq!(Model::NaiDiffusion5Full.vibe_model_key(), None);

    assert_eq!(Model::NaiDiffusion45Full.capabilities().params_version, 3);
    assert!(
        Model::NaiDiffusion45Full
            .capabilities()
            .supports_character_reference
    );
    assert!(
        !Model::NaiDiffusion4Full
            .capabilities()
            .supports_character_reference
    );
    assert!(!Model::NaiDiffusion3.capabilities().supports_characters());
    assert!(Model::NaiDiffusion3.capabilities().supports_smea);
    assert!(!Model::NaiDiffusion45Full.capabilities().supports_smea);
}

#[test]
fn character_set_counts_only_enabled_characters() {
    let set = CharacterSet::new(vec![
        Character {
            prompt: "a".to_owned(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: true,
        },
        Character {
            prompt: "b".to_owned(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: false,
        },
    ]);
    assert_eq!(set.enabled_count(), 1);
}

#[tokio::test]
async fn legacy_models_receive_smea_toggles() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .guidance(GuidanceParams {
            smea: true,
            smea_dyn: true,
            decrisp: true,
            variety_boost: false,
        })
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["sm"], json!(true));
    assert_eq!(body["parameters"]["sm_dyn"], json!(true));
    assert_eq!(body["parameters"]["dynamic_thresholding"], json!(true));
}

#[tokio::test]
async fn generate_does_not_parse_prompt_syntax_locally() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::with_transport(transport.clone());

    let result = client.generate(request("{bad prompt syntax")).await.unwrap();
    assert_eq!(result.images.len(), 1);
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/generate-image")
    );
}

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

#[tokio::test]
async fn credit_observer_receives_paired_events_without_prompt_text() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());

    let mut params = request_for("a very specific secret prompt", Model::NaiDiffusion45Full);
    params.core.seed = 5;
    let _ = client.generate(params).await.unwrap();

    let events = observer.events.lock().unwrap().clone();
    assert_eq!(events.len(), 2);

    let CreditEvent::RequestPlanned {
        fingerprint: planned,
        estimate,
    } = &events[0]
    else {
        panic!("expected a planned event, got {:?}", events[0]);
    };
    assert_eq!(planned.operation, ObservedOperation::GenerateImage);
    assert_eq!(planned.model, Some(Model::NaiDiffusion45Full));
    assert!(planned.prompt_len_chars > 0);
    assert!(estimate.total > 0);
    assert!(!format!("{planned:?}").contains("secret prompt"));

    let CreditEvent::RequestSettled {
        fingerprint: settled,
        outcome,
    } = &events[1]
    else {
        panic!("expected a settled event, got {:?}", events[1]);
    };
    assert_eq!(settled.request_id, planned.request_id);
    assert_eq!(*outcome, RequestOutcome::Succeeded { image_count: 1 });
}

#[tokio::test]
async fn credit_observer_reports_local_validation_failures() {
    let transport = MockTransport::default();
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());

    // Local validation runs before observation, so a rejected request emits nothing at all.
    let _ = client.generate(request("  ")).await.unwrap_err();
    assert!(observer.events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn credit_observer_reports_transport_failures() {
    let transport = MockTransport::default();
    transport.set_bytes_error(BridgeError::InvalidRequest(InvalidRequest::EmptyField {
        field: "simulated".to_owned(),
    }));
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::with_transport(transport).with_credit_observer(observer.clone());

    let _ = client.generate(request("1girl")).await.unwrap_err();

    let events = observer.events.lock().unwrap().clone();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Failed { .. },
            ..
        }
    ));
}
