use super::*;

#[tokio::test]
async fn generate_builds_baseline_text2img_request_shape() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn generate_with_auto_seed_returns_resolved_request_seed_without_faking_image_seed() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport), ClientConfig::default());

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
    let invalid_comment = crate::support::create_png_with_comment_text("{broken JSON");
    let transport = MockTransport::with_bytes_response(create_zip_with_pngs(&[
        ("missing.png", valid_without_metadata),
        ("invalid-comment.png", invalid_comment),
    ]));
    let client = Client::new(Arc::new(transport), ClientConfig::default());

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
