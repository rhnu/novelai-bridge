use std::sync::{Arc, Mutex};

use super::*;
use crate::{Character, CharacterPosition, Model, QualityPreset, TokenEncoder, TokenId, UcPreset};

#[derive(Default)]
struct RecordingEncoder {
    calls: Mutex<Vec<(String, FramingPolicy)>>,
}

impl TokenEncoder for RecordingEncoder {
    fn encode(&self, text: &str, framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError> {
        self.calls.lock().unwrap().push((text.to_owned(), framing));
        let mut tokens: Vec<TokenId> = text.chars().map(u32::from).collect();
        if framing.includes_framing() {
            tokens.push(0);
        }
        Ok(tokens)
    }

    fn vocabulary_size(&self) -> usize { 128 }
}

#[test]
fn counting_uses_effective_fields_and_preserves_character_indices() {
    let encoder = Arc::new(RecordingEncoder::default());
    let mut tokenizers = Tokenizers::new();
    let loaded: Arc<dyn TokenEncoder> = Arc::<RecordingEncoder>::clone(&encoder);
    assert!(
        tokenizers
            .register(ImageTokenizerKind::Qwen, loaded)
            .is_none()
    );
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion5Full)
        .negative_prompt("bad")
        .quality(QualityPreset::None)
        .uc_preset(UcPreset::None)
        .characters(vec![
            Character {
                prompt: "ignored".to_owned(),
                negative_prompt: None,
                position: CharacterPosition::default(),
                enabled: false,
            },
            Character {
                prompt: "dog".to_owned(),
                negative_prompt: Some("blur".to_owned()),
                position: CharacterPosition::default(),
                enabled: true,
            },
        ])
        .build();
    let usage = request.count_tokens(&tokenizers).unwrap();
    assert_eq!(usage.prompt.used, 4);
    assert_eq!(usage.negative_prompt.used, 4);
    assert_eq!(usage.characters[0].index, 1);
    assert_eq!(usage.characters[0].prompt.used, 4);
    assert_eq!(usage.characters[0].negative_prompt.used, 5);
    assert_eq!(
        *encoder.calls.lock().unwrap(),
        vec![
            ("cat".to_owned(), FramingPolicy::ModelInput),
            ("bad".to_owned(), FramingPolicy::ModelInput),
            ("dog".to_owned(), FramingPolicy::ModelInput),
            ("blur".to_owned(), FramingPolicy::ModelInput)
        ]
    );
}

#[test]
fn field_limits_are_independent_and_inclusive() {
    let usage = PromptTokenUsage {
        tokenizer: ImageTokenizerKind::Qwen,
        limit: 3,
        prompt: TokenCount::new(3, 3),
        negative_prompt: TokenCount::new(4, 3),
        characters: vec![CharacterTokenUsage {
            index: 2,
            prompt: TokenCount::new(1, 3),
            negative_prompt: TokenCount::new(5, 3),
        }],
    };
    let fields: Vec<PromptField> = usage.over_limit().map(|(field, _)| field).collect();
    assert_eq!(
        fields,
        [
            PromptField::NegativePrompt,
            PromptField::Character {
                index: 2,
                negative: true
            }
        ]
    );
    assert_eq!(TokenCount::new(4, 3).remaining(), 0);
}

#[test]
fn token_fields_use_editable_request_locations() {
    assert_eq!(
        PromptField::Prompt.generation_field(),
        GenerationField::Prompt
    );
    assert_eq!(
        PromptField::NegativePrompt.generation_field(),
        GenerationField::NegativePrompt
    );
    let field = PromptField::Character {
        index: 3,
        negative: true,
    };
    assert_eq!(
        field.generation_field(),
        GenerationField::Character {
            index: 3,
            field: CharacterField::NegativePrompt
        }
    );
    assert_eq!(field.label(), "characters.characters[3].negative_prompt");
}
