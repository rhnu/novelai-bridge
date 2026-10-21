use std::sync::Arc;

use super::*;
use crate::{FramingPolicy, TokenId};

struct CharacterEncoder;

impl TokenEncoder for CharacterEncoder {
    fn encode(&self, text: &str, _framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError> {
        Ok(text.chars().map(u32::from).collect())
    }

    fn vocabulary_size(&self) -> usize {
        128
    }
}

#[test]
fn registry_accepts_loaded_encoders_and_selects_by_model() {
    let mut tokenizers = Tokenizers::new();
    assert!(
        tokenizers
            .register(ImageTokenizerKind::Qwen, Arc::new(CharacterEncoder))
            .is_none()
    );
    let encoder = tokenizers
        .require_for_model(Model::NaiDiffusion5Full)
        .unwrap();
    assert_eq!(
        encoder.encode("ab", FramingPolicy::ContentOnly).unwrap(),
        [97, 98]
    );
    assert_eq!(
        encoder.count("猫ab", FramingPolicy::ContentOnly).unwrap(),
        3
    );
    assert_eq!(encoder.vocabulary_size(), 128);
    assert!(
        tokenizers
            .register(ImageTokenizerKind::Qwen, Arc::new(CharacterEncoder))
            .is_some()
    );
}

#[test]
fn missing_encoder_is_reported_rather_than_guessed() {
    let tokenizers = Tokenizers::new();
    assert!(tokenizers.is_empty());
    assert!(matches!(
        tokenizers.require_for_model(Model::NaiDiffusion5Full),
        Err(TokenizerError::TokenizerNotLoaded {
            kind: ImageTokenizerKind::Qwen
        })
    ));
}
