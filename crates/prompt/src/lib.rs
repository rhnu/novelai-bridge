//! Lossless `NovelAI` prompt syntax, independent of the SDK, network and tokenizers.
mod analysis;
mod lex;
mod types;
mod weight;

pub use analysis::analyze_prompt;
pub use lex::{
    PromptLexicalState, is_character_separator, is_randomizer_delimiter, is_text_rendering_marker,
};
pub use types::*;
