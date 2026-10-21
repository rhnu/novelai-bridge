use super::*;

#[tokio::test]
async fn img2img_request_uses_img2img_action_and_flat_fields() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn generate_normalizes_character_reference_image_payloads() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let _ = client.generate(request("1girl")).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert!(body["parameters"]["normalize_reference_strength_multiple"].is_null());
    assert!(body["parameters"]["reference_image_multiple"].is_null());
}

#[tokio::test]
async fn infill_drops_vibe_transfer_to_avoid_server_error() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn generate_clamps_reference_numeric_values_when_not_strict() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
