#![cfg(feature = "client")]

mod support;

use std::sync::Arc;
#[cfg(feature = "credit-observer")]
use std::sync::Mutex;

#[cfg(feature = "credit-observer")]
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
#[cfg(feature = "streaming")]
use futures_util::StreamExt;
use novelai_bridge::{
    BridgeError, Character, CharacterPosition, CharacterReference,
    CharacterReferenceType, CharacterSet, Client, EncodedImage,
    GenerateImageRequest, GeneratedImageMetadata, GuidanceParams,
    ImageFormat, ImageSize, Img2ImgRequest, InvalidRequest, Model,
    QualityPreset, ReferenceSet, UcPreset,
    VibeEncoding, VibeReference, VibeTransferConfig,
};
#[cfg(feature = "streaming")]
use novelai_bridge::{ClientConfig, GenerateImageStreamRequest, ImageStreamChunk};
#[cfg(feature = "credit-observer")]
use novelai_bridge::{CreditEvent, CreditObserver, ObservedOperation, RequestCostEstimate, RequestOutcome};
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

fn png_data_url(width: u32, height: u32) -> EncodedImage {
    EncodedImage::from_base64(&create_png_base64(width, height)).unwrap()
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let mut params = request_for("1girl", Model::NaiDiffusion3);
    params.core.seed = 42;
    let resolved = params.resolve_prompts().unwrap();
    let expected_prompt = resolved.prompt.clone();
    let expected_negative_prompt = resolved.negative_prompt.clone();

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = request_for("1girl", Model::NaiDiffusion5Full);
    let _ = client.generate(params).await.unwrap();

    let body = transport.last_body().expect("generate body");
    assert_eq!(body["model"], json!("nai-diffusion-5-full"));
    assert_eq!(body["parameters"]["params_version"], json!(4));
    assert_eq!(body["parameters"]["legacy_uc"], json!(false));
    assert_eq!(
        body["parameters"]["tag_hint_transparent_background"],
        json!(false)
    );
    assert!(body["parameters"]["v4_prompt"].is_object());
    // V5 has no legacy SMEA or decrisp parameters.
    assert!(body["parameters"]["sm"].is_null());
    assert!(body["parameters"]["sm_dyn"].is_null());
    assert!(body["parameters"]["dynamic_thresholding"].is_null());
}

#[tokio::test]
async fn generate_omits_v5_extensions_for_v45() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let mut img2img = Img2ImgRequest::new(png_data_url(400, 600), 1.0, 0.1);
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
            - 1.0)
            .abs()
            < 1e-6
    );
}

#[tokio::test]
async fn infill_uses_inpainting_model_for_each_family() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    for (model, expected) in [
        (Model::NaiDiffusion5Full, "nai-diffusion-5-full-inpainting"),
        (
            Model::NaiDiffusion4Curated,
            "nai-diffusion-4-curated-inpainting",
        ),
        (Model::NaiDiffusion3, "nai-diffusion-3-inpainting"),
        (
            Model::NaiDiffusion3Furry,
            "nai-diffusion-furry-3-inpainting",
        ),
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let mut img2img = Img2ImgRequest::new(png_data_url(300, 100), 0.7, 0.0);
    img2img.mask = Some(png_data_url(50, 50));
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .size(ImageSize::square())
        .img2img(img2img)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");

    assert_eq!(
        decoded_dimensions(&body["parameters"]["image"]),
        (1024, 1024)
    );
    assert_eq!(
        decoded_dimensions(&body["parameters"]["mask"]),
        (1024, 1024)
    );
}


#[tokio::test]
async fn generate_with_auto_seed_returns_resolved_request_seed_without_faking_image_seed() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport), Default::default());

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
    let client = Client::new(Arc::new(transport), Default::default());

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
    let invalid_comment = support::create_png_with_comment_text("{broken JSON");
    let transport = MockTransport::with_bytes_response(create_zip_with_pngs(&[
        ("missing.png", valid_without_metadata),
        ("invalid-comment.png", invalid_comment),
    ]));
    let client = Client::new(Arc::new(transport), Default::default());

    let result = client.generate(request("1girl")).await.unwrap();

    assert_eq!(result.images.len(), 2);
    assert!(matches!(
        result.images[0].metadata,
        GeneratedImageMetadata::NotPresent
    ));
    assert!(matches!(
        &result.images[1].metadata,
        GeneratedImageMetadata::Parsed(metadata)
            if metadata.warnings.contains(&novelai_bridge::PngMetadataWarning::InvalidCommentJson)
    ));
}

#[tokio::test]
#[cfg(feature = "streaming")]
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
#[cfg(feature = "streaming")]
async fn generate_stream_uses_custom_image_base_url() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![]);
    let client = Client::new(
        Arc::new(transport.clone()),
        ClientConfig { image_base_url: "https://image.example.test/root/".to_owned() },
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
#[cfg(feature = "streaming")]
async fn generate_stream_rejects_v3_models_before_transport() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let transport = MockTransport::default();
        let client = Client::new(Arc::new(transport.clone()), Default::default());
        let params = GenerateImageStreamRequest {
            base: GenerateImageRequest::builder("1girl", model).build(),
            ..Default::default()
        };

        let Err(error) = client.generate_stream(params).await else {
            panic!("V3 models must not start a stream");
        };
        assert!(matches!(
            error,
            BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
                if feature == "delivery.stream"
        ));
        assert!(transport.last_path().is_none());
    }
}

#[tokio::test]
async fn generate_normalizes_character_reference_image_payloads() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
            if feature == "references.character_references"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_drops_character_references_on_v4_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .vibe_transfer(VibeTransferConfig {
            references: vec![VibeReference {
                vibe_data_cache: VibeEncoding::from_base64("QUJDRA==").unwrap(),
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
            if feature == "references.vibe_transfer"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_serializes_explicit_vibe_data_cache() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    transport.set_encode_vibe_response(vec![1_u8, 2, 3]);
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 0.6,
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let _ = client.generate(request("1girl")).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["normalize_reference_strength_multiple"].is_null());
    assert!(body["parameters"]["reference_image_multiple"].is_null());
}



#[tokio::test]
async fn character_positions_enable_use_coords_when_non_center_and_not_explicit() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport), Default::default());

    let mut params = request_for("1girl", Model::NaiDiffusion45Full);
    params.strict_mode = true;
    params.references = Some(ReferenceSet {
        vibe_transfer: Some(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 1.0,
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
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
        }) if left == "references.vibe_transfer" && right == "references.character_references"
    ));
}

#[tokio::test]
async fn infill_drops_vibe_transfer_to_avoid_server_error() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let mut img2img = Img2ImgRequest::new(png_data_url(128, 128), 0.6, 0.0);
    img2img.mask = Some(png_data_url(128, 128));
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .img2img(img2img)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 1.0,
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let error = client.generate(request("   ")).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if field == "core.prompt"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_rejects_invalid_scale_locally() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::default())
        .scale(11.0)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if field == "sampling.scale"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_clamps_invalid_numeric_values_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 2.0,
            references: vec![VibeReference {
                strength: -1.0,
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .vibe_transfer(VibeTransferConfig {
            strength: 1.0,
            references: vec![VibeReference {
                strength: 5.0,
                vibe_data_cache: VibeEncoding::from_base64("AQID").unwrap(),
            }],
        })
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if field == "references.vibe_transfer.references[0].strength"
    ));
    assert!(transport.last_path().is_none());
}





#[tokio::test]
async fn quality_none_still_reports_the_toggle_state() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .quality(QualityPreset::Light)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "core.quality"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn transparent_background_appends_the_tag_and_sets_the_wire_hint() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .transparent_background(true)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    // The tag precedes the quality tags, matching the prompt the web client assembles.
    assert_eq!(
        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
        json!("1girl, transparent background, very aesthetic, masterpiece, no text")
    );
    assert_eq!(
        body["parameters"]["tag_hint_transparent_background"],
        json!(true)
    );
}

#[tokio::test]
async fn transparent_background_hint_tracks_the_request_value() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let _ = client
        .generate(request_for("1girl", Model::NaiDiffusion5Full))
        .await
        .unwrap();

    let body = transport.last_body().expect("generate body");
    assert_eq!(
        body["parameters"]["tag_hint_transparent_background"],
        json!(false)
    );
}

#[tokio::test]
async fn transparent_background_is_dropped_off_v5_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .transparent_background(true)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(
        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
        json!("1girl, very aesthetic, masterpiece, no text")
    );
    assert!(body["parameters"]["tag_hint_transparent_background"].is_null());
}

#[tokio::test]
async fn transparent_background_is_rejected_off_v5_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .transparent_background(true)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "core.transparent_background"
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn furry_mode_prepends_the_dataset_tag_for_v4_and_later_models() {
    for model in [
        Model::NaiDiffusion4Full,
        Model::NaiDiffusion4Curated,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion45Curated,
        Model::NaiDiffusion5Full,
        Model::NaiDiffusion5Curated,
    ] {
        let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
        let client = Client::new(Arc::new(transport.clone()), Default::default());
        let params = GenerateImageRequest::builder("1girl", model)
            .furry_mode(true)
            .build();

        client.generate(params).await.unwrap();
        let body = transport.last_body().expect("generate body");
        assert_eq!(
            body["parameters"]["v4_prompt"]["caption"]["base_caption"],
            json!("fur dataset, 1girl, very aesthetic, masterpiece, no text"),
            "{model:?}"
        );
    }
}

#[tokio::test]
async fn furry_mode_is_dropped_for_v3_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .furry_mode(true)
        .build();

    client.generate(params).await.unwrap();
    assert_eq!(
        transport.last_body().unwrap()["parameters"]["prompt"],
        json!("1girl, very aesthetic, masterpiece, no text")
    );
}

#[tokio::test]
async fn furry_mode_is_rejected_for_v3_in_strict_mode() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), Default::default());
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .furry_mode(true)
        .strict_mode(true)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if feature == "core.furry_mode"
    ));
    assert!(transport.last_path().is_none());
}




#[tokio::test]
async fn legacy_models_receive_smea_toggles() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), Default::default());

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
    let client = Client::new(Arc::new(transport.clone()), Default::default());

    let result = client
        .generate(request("{bad prompt syntax"))
        .await
        .unwrap();
    assert_eq!(result.images.len(), 1);
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/generate-image")
    );
}

#[cfg(feature = "credit-observer")]
#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<CreditEvent>>,
}

#[cfg(feature = "credit-observer")]
#[async_trait]
impl CreditObserver for RecordingObserver {
    async fn observe(&self, event: CreditEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[tokio::test]
#[cfg(feature = "credit-observer")]
async fn credit_observer_receives_paired_events_without_prompt_text() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), Default::default()).with_credit_observer(observer.clone());

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
    let ObservedOperation::GenerateImage(summary) = &planned.operation else { panic!("expected generation summary"); };
    assert_eq!(summary.pricing.model, Model::NaiDiffusion45Full);
    assert!(summary.prompt_len_chars > 0);
    assert!(matches!(estimate, RequestCostEstimate::Generation(value) if value.total > 0));
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
#[cfg(feature = "credit-observer")]
async fn credit_observer_reports_local_validation_failures() {
    let transport = MockTransport::default();
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), Default::default()).with_credit_observer(observer.clone());

    // Local validation runs before observation, so a rejected request emits nothing at all.
    let _ = client.generate(request("  ")).await.unwrap_err();
    assert!(observer.events.lock().unwrap().is_empty());
}

#[tokio::test]
#[cfg(feature = "credit-observer")]
async fn credit_observer_reports_transport_failures() {
    let transport = MockTransport::default();
    transport.set_bytes_error(novelai_bridge::TransportError::new(
        novelai_bridge::TransportOperation::SendRequest,
        None,
        "simulated".into(),
    ));
    let observer = Arc::new(RecordingObserver::default());
    let client = Client::new(Arc::new(transport), Default::default()).with_credit_observer(observer.clone());

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

/// Builds a request whose every prompt source contributes text, so a wire field that re-derives
/// prompt text instead of reading `resolve_prompts()` cannot match by accident.
fn request_with_every_prompt_source(model: Model) -> GenerateImageRequest {
    let mut params = GenerateImageRequest::builder("1girl", model)
        .negative_prompt("extra tags")
        .quality(QualityPreset::Standard)
        .uc_preset(UcPreset::Heavy)
        .build();
    params.core.transparent_background = model.descriptor().prompt().transparent_background;
    if model.descriptor().prompt().characters.is_some() {
        params.characters = Some(CharacterSet::new(vec![
            Character {
                prompt: "girl, blue hair".to_owned(),
                negative_prompt: Some("bad hands".to_owned()),
                position: CharacterPosition { x: 0.1, y: 0.9 },
                enabled: true,
            },
            Character {
                prompt: "boy, red hair".to_owned(),
                negative_prompt: None,
                position: CharacterPosition::default(),
                enabled: false,
            },
        ]));
    }
    params
}

/// Guards the single prompt-assembly source: whatever `resolve_prompts` reports must be byte-for-
/// byte what the request sends. If these ever diverge, token counting silently under-reports.
#[tokio::test]
async fn resolved_prompts_match_the_wire_payload_verbatim() {
    for model in [
        Model::NaiDiffusion3,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion5Full,
    ] {
        let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
        let client = Client::new(Arc::new(transport.clone()), Default::default());

        let params = request_with_every_prompt_source(model);
        let resolved = params.resolve_prompts().unwrap();
        client.generate(params).await.unwrap();

        let body = transport.last_body().expect("generate body");
        let parameters = &body["parameters"];

        assert_eq!(
            parameters["negative_prompt"],
            json!(resolved.negative_prompt),
            "{model:?} negative_prompt"
        );

        if model.descriptor().wire().uses_structured_prompt() {
            assert_eq!(
                parameters["v4_prompt"]["caption"]["base_caption"],
                json!(resolved.prompt),
                "{model:?} v4 base caption"
            );
            assert_eq!(
                parameters["v4_negative_prompt"]["caption"]["base_caption"],
                json!(resolved.negative_prompt),
                "{model:?} v4 negative base caption"
            );

            // Both wire representations carry only enabled characters.
            let wire_characters = parameters["characterPrompts"]
                .as_array()
                .expect("characterPrompts");
            assert_eq!(wire_characters.len(), resolved.characters.len());
            for (wire, resolved) in wire_characters.iter().zip(&resolved.characters) {
                assert_eq!(wire["prompt"], json!(resolved.prompt));
                assert_eq!(wire["uc"], json!(resolved.negative_prompt));
                assert_eq!(wire["enabled"], json!(true));
            }

            let enabled: Vec<_> = resolved.characters.iter().collect();
            let captions = parameters["v4_prompt"]["caption"]["char_captions"]
                .as_array()
                .expect("char_captions");
            assert_eq!(captions.len(), enabled.len());
            for (caption, resolved) in captions.iter().zip(&enabled) {
                assert_eq!(caption["char_caption"], json!(resolved.prompt));
            }
            let negative_captions = parameters["v4_negative_prompt"]["caption"]["char_captions"]
                .as_array()
                .expect("negative char_captions");
            for (caption, resolved) in negative_captions.iter().zip(&enabled) {
                assert_eq!(caption["char_caption"], json!(resolved.negative_prompt));
            }
        } else {
            assert_eq!(
                parameters["prompt"],
                json!(resolved.prompt),
                "{model:?} prompt"
            );
            assert_eq!(
                parameters["uc"],
                json!(resolved.negative_prompt),
                "{model:?} uc"
            );
            assert!(
                resolved.characters.is_empty(),
                "{model:?} has no character support"
            );
        }
    }
}

/// The assembled prompt must be strictly longer than the caller's raw text whenever a preset
/// contributes, which is exactly the case token counting would otherwise under-report.

