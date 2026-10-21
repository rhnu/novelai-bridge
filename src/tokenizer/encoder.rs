use super::{FramingPolicy, TokenId, TokenizerError};

/// Internal destination for encoding or counting without retaining the complete output.
pub(super) trait TokenSink {
    fn push(&mut self, id: TokenId);

    fn extend_from_slice(&mut self, ids: &[TokenId]);

    /// T5 reconstructs its selected path from the end before appending framing.
    fn reverse(&mut self);
}

impl TokenSink for Vec<TokenId> {
    fn push(&mut self, id: TokenId) { Self::push(self, id); }

    fn extend_from_slice(&mut self, ids: &[TokenId]) { Self::extend_from_slice(self, ids); }

    fn reverse(&mut self) { self.as_mut_slice().reverse(); }
}

#[derive(Default)]
pub(super) struct TokenCounter(pub(super) u32);

impl TokenSink for TokenCounter {
    fn push(&mut self, _id: TokenId) { self.0 = self.0.saturating_add(1); }

    fn extend_from_slice(&mut self, ids: &[TokenId]) {
        self.0 = self
            .0
            .saturating_add(u32::try_from(ids.len()).unwrap_or(u32::MAX));
    }

    fn reverse(&mut self) {}
}

/// A loaded prompt encoder independent of request planning and asset storage.
pub trait TokenEncoder: Send + Sync {
    /// Encodes text into an ordered sequence, without a partial result on failure.
    ///
    /// # Errors
    /// Returns an encoding error when input processing fails.
    fn encode(&self, text: &str, framing: FramingPolicy) -> Result<Vec<TokenId>, TokenizerError>;

    /// Counts the same sequence returned by encoding.
    ///
    /// # Errors
    /// Returns the same errors as encoding.
    fn count(&self, text: &str, framing: FramingPolicy) -> Result<u32, TokenizerError> {
        Ok(u32::try_from(self.encode(text, framing)?.len()).unwrap_or(u32::MAX))
    }

    /// Number of entries in this encoder's vocabulary, independent of prompt limits.
    fn vocabulary_size(&self) -> usize;
}
