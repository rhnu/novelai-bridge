use super::{FramingPolicy, TokenId, TokenizerError};

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
