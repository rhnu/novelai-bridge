#![cfg(feature = "bundled-assets")]

mod support;

use novelai_bridge::{
    BridgeError, Character, CharacterSet, Client, FramingPolicy, GenerateImageRequest,
    GenerateImageStreamRequest, ImageTokenizerKind, InvalidRequest, Model, QualityPreset,
    TokenCount, TokenizerError, Tokenizers, UcPreset,
};
use std::sync::{Arc, LazyLock};
use support::MockTransport;

static TOKENIZERS: LazyLock<Arc<Tokenizers>> =
    LazyLock::new(|| Arc::new(Tokenizers::bundled().unwrap()));

fn request(model: Model) -> GenerateImageRequest {
    GenerateImageRequest::builder("hello", model)
        .quality(QualityPreset::None)
        .uc_preset(UcPreset::None)
        .seed(42)
        .build()
}

#[test]
fn bundled_registries_share_validated_encoders() {
    let second = Tokenizers::bundled().unwrap();
    for kind in [
        ImageTokenizerKind::Clip,
        ImageTokenizerKind::T5,
        ImageTokenizerKind::Qwen,
    ] {
        assert!(std::ptr::eq(
            TOKENIZERS.get(kind).unwrap(),
            second.get(kind).unwrap()
        ));
    }
}

#[test]
fn all_counting_entry_points_include_presets_and_use_the_same_framing() {
    let params = GenerateImageRequest::builder("hello", Model::NaiDiffusion5Full)
        .negative_prompt("bad")
        .quality(QualityPreset::Standard)
        .uc_preset(UcPreset::Heavy)
        .transparent_background(true)
        .build();
    let usage = params.count_tokens(&TOKENIZERS).unwrap();
    let encoder = TOKENIZERS.for_model(params.core.model).unwrap();
    assert!(
        usage.prompt.used
            > encoder
                .count(&params.core.prompt, FramingPolicy::ModelInput)
                .unwrap()
    );
    assert!(usage.negative_prompt.used > encoder.count("bad", FramingPolicy::ModelInput).unwrap());
    let resolved = params.resolve_prompts().unwrap();
    assert_eq!(
        usage.prompt.used,
        encoder
            .count(&resolved.prompt, FramingPolicy::ModelInput)
            .unwrap()
    );
    assert_eq!(
        usage.negative_prompt.used,
        encoder
            .count(&resolved.negative_prompt, FramingPolicy::ModelInput)
            .unwrap()
    );
    let stream = GenerateImageStreamRequest {
        base: params.clone(),
        ..Default::default()
    };
    assert_eq!(stream.count_tokens(&TOKENIZERS).unwrap(), usage);
    let client =
        Client::with_transport(MockTransport::default()).with_tokenizers(TOKENIZERS.clone());
    assert_eq!(client.count_tokens(&params).unwrap(), usage);
    assert_eq!(
        stream
            .count_tokens_with(&TOKENIZERS, FramingPolicy::ContentOnly)
            .unwrap(),
        params
            .count_tokens_with(&TOKENIZERS, FramingPolicy::ContentOnly)
            .unwrap()
    );
    for (model, extra) in [
        (Model::NaiDiffusion3, 2),
        (Model::NaiDiffusion4Full, 1),
        (Model::NaiDiffusion5Full, 0),
    ] {
        let params = request(model);
        assert_eq!(
            params.count_tokens(&TOKENIZERS).unwrap().prompt.used,
            params
                .count_tokens_with(&TOKENIZERS, FramingPolicy::ContentOnly)
                .unwrap()
                .prompt
                .used
                + extra
        );
    }
}

#[tokio::test]
async fn strict_limits_reject_each_field_before_transport() {
    for model in [
        Model::NaiDiffusion3,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion5Full,
        Model::NaiDiffusion5Curated,
    ] {
        for negative in [false, true] {
            let transport = MockTransport::default();
            let client =
                Client::with_transport(transport.clone()).with_tokenizers(TOKENIZERS.clone());
            let mut params = request(model);
            params.strict_mode = true;
            if negative {
                params.core.negative_prompt = Some("hello ".repeat(1600));
            } else {
                params.core.prompt = "hello ".repeat(1600);
            }
            let expected = if negative {
                "negative_prompt"
            } else {
                "prompt"
            };
            assert!(
                matches!(client.generate(params).await, Err(BridgeError::InvalidRequest(
                InvalidRequest::PromptTokenLimitExceeded { field, used, limit }
            )) if field == expected && used > limit && limit == model.descriptor().prompt().token_limit)
            );
            assert!(transport.last_body().is_none());
        }
    }
}

#[tokio::test]
async fn streaming_shares_the_same_limit_check_and_non_strict_keeps_text() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone()).with_tokenizers(TOKENIZERS.clone());
    let mut params = request(Model::NaiDiffusion5Curated);
    params.core.prompt = "hello ".repeat(1000);
    params.strict_mode = true;
    let stream = GenerateImageStreamRequest {
        base: params.clone(),
        ..Default::default()
    };
    assert!(matches!(
        client.generate_stream(stream).await,
        Err(BridgeError::InvalidRequest(
            InvalidRequest::PromptTokenLimitExceeded { .. }
        ))
    ));
    assert!(transport.last_body().is_none());
    params.strict_mode = false;
    client.generate(params.clone()).await.unwrap();
    assert_eq!(
        transport.last_body().unwrap()["parameters"]["v4_prompt"]["caption"]["base_caption"],
        params.core.prompt
    );
    let stream = GenerateImageStreamRequest {
        base: params.clone(),
        ..Default::default()
    };
    let _ = client.generate_stream(stream).await.unwrap();
    assert_eq!(
        transport.last_body().unwrap()["parameters"]["v4_prompt"]["caption"]["base_caption"],
        params.core.prompt
    );
}

#[tokio::test]
async fn enabled_characters_preserve_original_indices_and_positions() {
    let mut params = request(Model::NaiDiffusion5Curated);
    params.strict_mode = true;
    params.characters = Some(CharacterSet {
        use_coords: Some(true),
        characters: vec![
            Character {
                prompt: "ignored ".repeat(2000),
                enabled: false,
                negative_prompt: None,
                position: novelai_bridge::CharacterPosition::default(),
            },
            Character {
                prompt: "hello".into(),
                negative_prompt: Some("bad".into()),
                position: novelai_bridge::CharacterPosition { x: 0.3, y: 0.7 },
                enabled: true,
            },
        ],
    });
    let resolved = params.resolve_prompts().unwrap();
    assert_eq!(resolved.characters.len(), 1);
    assert_eq!(resolved.characters[0].index, 1);
    let usage = params.count_tokens(&TOKENIZERS).unwrap();
    assert_eq!(usage.characters.len(), 1);
    assert_eq!(usage.characters[0].index, 1);
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone()).with_tokenizers(TOKENIZERS.clone());
    client.generate(params.clone()).await.unwrap();
    let body = transport.last_body().unwrap();
    assert_eq!(
        body["parameters"]["characterPrompts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let center = body["parameters"]["characterPrompts"][0]["center"]["x"]
        .as_f64()
        .unwrap();
    assert!((center - 0.3).abs() < 0.00001);
    for negative in [false, true] {
        let mut params = params.clone();
        let character = &mut params.characters.as_mut().unwrap().characters[1];
        if negative {
            character.negative_prompt = Some("hello ".repeat(1000));
        } else {
            character.prompt = "hello ".repeat(1000);
        }
        let transport = MockTransport::default();
        let client = Client::with_transport(transport.clone()).with_tokenizers(TOKENIZERS.clone());
        let expected = if negative {
            "characters[1].negative_prompt"
        } else {
            "characters[1].prompt"
        };
        assert!(
            matches!(client.generate(params).await, Err(BridgeError::InvalidRequest(
            InvalidRequest::PromptTokenLimitExceeded { field, .. })) if field == expected)
        );
        assert!(transport.last_body().is_none());
    }
}

#[tokio::test]
async fn no_tokenizers_preserves_generation_but_explicit_missing_registry_errors() {
    let transport = MockTransport::default();
    let client = Client::with_transport(transport.clone());
    let mut params = request(Model::NaiDiffusion5Curated);
    params.core.prompt = "hello ".repeat(1000);
    params.strict_mode = true;
    assert!(matches!(
        client.count_tokens(&params),
        Err(BridgeError::Tokenizer(
            TokenizerError::TokenizerNotLoaded { .. }
        ))
    ));
    client.generate(params.clone()).await.unwrap();
    assert!(transport.last_body().is_some());
    let transport = MockTransport::default();
    let client =
        Client::with_transport(transport.clone()).with_tokenizers(Arc::new(Tokenizers::new()));
    assert!(matches!(
        client.generate(params).await,
        Err(BridgeError::Tokenizer(
            TokenizerError::TokenizerNotLoaded { .. }
        ))
    ));
    assert!(transport.last_body().is_none());
}

#[tokio::test]
async fn non_strict_normalization_and_tokenizer_injection_do_not_change_wire() {
    for model in [
        Model::NaiDiffusion3,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion5Full,
    ] {
        let mut params = request(model);
        params.core.quality = QualityPreset::Light;
        params.core.transparent_background = true;
        let expected = params.resolve_prompts().unwrap();
        let with = MockTransport::default();
        let without = MockTransport::default();
        Client::with_transport(with.clone())
            .with_tokenizers(TOKENIZERS.clone())
            .generate(params.clone())
            .await
            .unwrap();
        Client::with_transport(without.clone())
            .generate(params)
            .await
            .unwrap();
        assert_eq!(with.last_body(), without.last_body());
        let body = with.last_body().unwrap();
        let prompt = if model.descriptor().wire().uses_structured_prompt() {
            &body["parameters"]["v4_prompt"]["caption"]["base_caption"]
        } else {
            &body["parameters"]["prompt"]
        };
        assert_eq!(prompt, &expected.prompt);
    }
}

#[test]
fn token_count_limits_are_inclusive_and_saturating() {
    assert!(!TokenCount::new(703, 703).is_over_limit());
    assert!(TokenCount::new(704, 703).is_over_limit());
    assert_eq!(TokenCount::new(704, 703).remaining(), 0);
    assert_eq!(TokenCount::new(700, 703).remaining(), 3);
}

#[tokio::test]
async fn normalized_counts_and_prepared_send_share_the_effective_character_set() {
    let mut raw = request(Model::NaiDiffusion45Full);
    raw.characters = Some(CharacterSet::new(
        (0..9)
            .map(|index| Character {
                prompt: if index == 0 {
                    String::new()
                } else {
                    format!("hello {index}")
                },
                negative_prompt: Some("bad".to_owned()),
                position: novelai_bridge::CharacterPosition::default(),
                enabled: index != 0,
            })
            .collect(),
    ));
    let normalized = raw.clone().normalize().unwrap();
    let usage = normalized.count_tokens(&TOKENIZERS).unwrap();
    assert_eq!(usage, raw.count_tokens(&TOKENIZERS).unwrap());
    assert_eq!(
        usage.characters.iter().map(|c| c.index).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    let transport = MockTransport::default();
    Client::with_transport(transport.clone())
        .with_tokenizers(TOKENIZERS.clone())
        .generate_prepared(normalized.prepare().unwrap())
        .await
        .unwrap();
    let body = transport.last_body().unwrap();
    let captions = body["parameters"]["v4_prompt"]["caption"]["char_captions"]
        .as_array()
        .unwrap();
    assert_eq!(captions.len(), usage.characters.len());
    let encoder = TOKENIZERS.for_model(raw.core.model).unwrap();
    for (caption, count) in captions.iter().zip(&usage.characters) {
        assert_eq!(
            encoder
                .count(
                    caption["char_caption"].as_str().unwrap(),
                    FramingPolicy::ModelInput
                )
                .unwrap(),
            count.prompt.used
        );
    }
}
