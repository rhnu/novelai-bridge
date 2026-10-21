//! Local CLIP, T5 and Qwen prompt tokenizers.
//!
//! Each encoder is implemented from its public upstream format and consumes that format directly:
//! CLIP from the `OpenAI` merge table, T5 from a `SentencePiece` model, Qwen from `tokenizer.json`.
//! No tokenizer response from `NovelAI` is required at runtime, and no asset is fetched during an
//! encode operation.
//!
//! # Scope
//!
//! The tokenizer core is separate from prompt transformation. Weighting syntax, multiprompt
//! separators, quality and undesired-content presets and character-field aggregation are not
//! tokenizer operations. Request-level counting goes through
//! `GenerateImageRequest::resolve_prompts`, which
//! produces exactly the text the request will send.
//!
//! # Assets
//!
//! Assets are supplied by the caller as bytes, so the library works without network access and
//! asset upgrades stay auditable. Load them once into a [`Tokenizers`] registry and share it:
//!
//! ```no_run
//! use novelai_bridge::{FramingPolicy, Model, Tokenizers};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tokenizers = Tokenizers::new().with_t5_sentencepiece(&std::fs::read("spiece.model")?)?;
//!
//! let encoder = tokenizers.require_for_model(Model::NaiDiffusion45Full)?;
//! let used = encoder.count("1girl, blue hair", FramingPolicy::ModelInput);
//! # let _ = used;
//! # Ok(())
//! # }
//! ```

mod bpe;
#[cfg(feature = "bundled-assets")]
mod bundled;
mod clip;
mod encoder;
mod error;
mod profile;
mod qwen;
mod registry;
mod t5;

#[cfg(test)]
mod tests;

pub use clip::ClipTokenizer;
pub use encoder::TokenEncoder;
pub use error::{TokenizerAsset, TokenizerError};
pub use profile::{FramingPolicy, TokenCount, TokenId};
pub use qwen::QwenTokenizer;
pub use registry::Tokenizers;
pub use t5::T5Tokenizer;
