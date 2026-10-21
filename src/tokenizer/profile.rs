//! Shared tokenizer vocabulary types and framing policy.

use serde::{Deserialize, Serialize};

/// A single token identifier.
///
/// Wide enough for the largest supported vocabulary; the pinned Qwen profile has about 248k entries.
pub type TokenId = u32;

/// Whether an encoded sequence carries the model-input framing tokens.
///
/// A model wraps its conditioning text in start/end markers, but a prompt editor usually wants to
/// show the content length alone. The choice belongs next to the model capability mapping rather
/// than inside a generic BPE or Unigram implementation, so it is always explicit.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FramingPolicy {
    /// Encode the content only, with no start/end markers.
    ContentOnly,
    /// Add the framing tokens the model itself uses: CLIP start/end, T5 end-of-sequence.
    ///
    /// This is the default because it matches what the model actually consumes, and therefore
    /// what a prompt limit is measured against.
    #[default]
    ModelInput,
}

impl FramingPolicy {
    /// Returns `true` when model-input framing tokens should be added.
    #[must_use]
    pub const fn includes_framing(self) -> bool { matches!(self, Self::ModelInput) }
}

/// Token usage for one prompt field against its model's limit.
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenCount {
    /// Number of tokens the encoded text produced under the selected framing policy.
    pub used: u32,
    /// The selected model's [`crate::PromptProfile::token_limit`].
    pub limit: u32,
}

impl TokenCount {
    /// Creates a count.
    #[must_use]
    pub const fn new(used: u32, limit: u32) -> Self { Self { used, limit } }

    /// Returns `true` when the text exceeds the model's prompt limit.
    #[must_use]
    pub const fn is_over_limit(&self) -> bool { self.used > self.limit }

    /// Returns how many tokens remain, saturating at zero when over the limit.
    #[must_use]
    pub const fn remaining(&self) -> u32 { self.limit.saturating_sub(self.used) }
}
