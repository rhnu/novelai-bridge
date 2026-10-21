use super::*;

#[tokio::test]
async fn generate_rejects_empty_prompt_locally() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let error = client.generate(request("   ")).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
            if *field == RequestField::Generation(GenerationField::Prompt)
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_rejects_invalid_scale_locally() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageRequest::builder("1girl", Model::default())
        .scale(11.0)
        .policy(ValidationPolicy::Reject)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::NumericOutOfRange { ref field, .. })
            if *field == RequestField::Generation(GenerationField::Scale)
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn generate_clamps_invalid_numeric_values_under_normalize_policy() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let mut params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .scale(11.0)
        .size(ImageSize {
            width: 17,
            height: 1599,
        })
        .image_to_image(ImageToImageInput::new(png_data_url(64, 64), 5.0, -0.5))
        .build();
    params.sampling.cfg_rescale = -0.5;
    params.core.n_samples = 9;

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["scale"], json!(10.0));
    assert_eq!(body["parameters"]["cfg_rescale"], json!(0.0));
    assert_eq!(body["parameters"]["n_samples"], json!(8));
    assert_eq!(body["parameters"]["width"], json!(64));
    assert_eq!(body["parameters"]["height"], json!(1600));
    assert!((body["parameters"]["strength"].as_f64().unwrap() - 0.99).abs() < 1e-6);
    assert_eq!(body["parameters"]["noise"], json!(0.0));
}

#[tokio::test]
async fn quality_none_reports_the_effective_hint() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion5Full)
        .quality(QualityPreset::None)
        .build();

    let _ = client.generate(params).await.unwrap();
    let body = transport.last_body().expect("generate body");
    assert_eq!(body["parameters"]["tag_hint_qt"], json!(0));
    assert_eq!(
        body["parameters"]["v4_prompt"]["caption"]["base_caption"],
        json!("1girl")
    );
}

#[tokio::test]
async fn light_quality_preset_falls_back_to_standard_off_v5() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn light_quality_preset_is_rejected_off_v5_under_reject_policy() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .quality(QualityPreset::Light)
        .policy(ValidationPolicy::Reject)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if *feature == RequestField::Generation(GenerationField::Quality)
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn transparent_background_appends_the_tag_and_sets_the_wire_hint() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn transparent_background_is_dropped_off_v5_under_normalize_policy() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn transparent_background_is_rejected_off_v5_under_reject_policy() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion45Full)
        .transparent_background(true)
        .policy(ValidationPolicy::Reject)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if *feature == RequestField::Generation(GenerationField::TransparentBackground)
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
        let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
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
async fn furry_mode_is_dropped_for_v3_under_normalize_policy() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
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
async fn furry_mode_is_rejected_for_v3_under_reject_policy() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .furry_mode(true)
        .policy(ValidationPolicy::Reject)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if *feature == RequestField::Generation(GenerationField::FurryMode)
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn legacy_models_receive_smea_toggles() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
