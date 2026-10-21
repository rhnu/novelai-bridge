use super::*;

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
        let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

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
