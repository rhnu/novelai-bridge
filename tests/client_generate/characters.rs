use super::*;

#[tokio::test]
async fn character_positions_enable_use_coords_when_non_center_and_not_explicit() {
    let transport = MockTransport::with_bytes_response(create_zip_with_single_png());
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
async fn characters_are_rejected_on_v3_under_reject_policy() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageRequest::builder("1girl", Model::NaiDiffusion3)
        .characters(vec![Character {
            prompt: "char-a".to_owned(),
            negative_prompt: None,
            position: CharacterPosition::default(),
            enabled: true,
        }])
        .policy(ValidationPolicy::Reject)
        .build();

    let error = client.generate(params).await.unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
            if *feature == RequestField::Generation(GenerationField::Characters)
    ));
    assert!(transport.last_path().is_none());
}

#[tokio::test]
async fn validate_rejects_vibe_transfer_and_character_references_together() {
    let transport = MockTransport::default();
    let client = Client::new(Arc::new(transport), ClientConfig::default());

    let mut params = request_for("1girl", Model::NaiDiffusion45Full);
    params.policy = ValidationPolicy::Reject;
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
        }) if *left == RequestField::Generation(GenerationField::VibeTransfer) && *right == RequestField::Generation(GenerationField::CharacterReferences)
    ));
}
