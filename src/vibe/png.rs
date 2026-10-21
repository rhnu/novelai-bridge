use base64::{Engine, engine::general_purpose::STANDARD};

use super::VibeDocument;
use crate::{BridgeError, MetadataError};

impl VibeDocument {
    /// Reads and validates the official document embedded in a PNG's `naidata` text.
    ///
    /// # Errors
    /// Returns complete PNG validation errors or the original document parsing error.
    pub fn from_png(bytes: &[u8]) -> Result<Option<Self>, BridgeError> {
        crate::extract_embedded_vibe_document_from_png_bytes(bytes)?
            .map(|raw| Self::from_json(raw.as_bytes()).map_err(document_error))
            .transpose()
    }

    /// Embeds this validated document without re-encoding the PNG's pixels or other chunks.
    ///
    /// # Errors
    /// Returns document, PNG validation, text encoding or resource-budget errors.
    pub fn embed_in_png(&self, bytes: &[u8]) -> Result<Vec<u8>, BridgeError> {
        let policy = crate::ImageDecodePolicy::default();
        let text_limit = policy.max_text_chunk_bytes.min(policy.max_total_text_bytes);
        let mut limits = super::VibeDocumentLimits::default();
        limits.max_document_bytes = limits.max_document_bytes.min(text_limit / 4 * 3);
        let json = self.to_json_with_limits(limits).map_err(document_error)?;
        let encoded_bytes = (json.len() as u64).div_ceil(3).saturating_mul(4);
        for limit in [
            crate::ImageLimit::TextChunkBytes,
            crate::ImageLimit::TotalTextBytes,
            crate::ImageLimit::EncodedBytes,
        ] {
            policy
                .check(limit, encoded_bytes)
                .map_err(|source| MetadataError::InvalidPngText {
                    source: Box::new(source),
                })?;
        }
        let text = STANDARD.encode(json);
        crate::metadata::png::write_png_text_chunks(
            bytes,
            &[crate::metadata::png::PngTextEntry::new("naidata", text)],
        )
    }
}

fn document_error(source: super::VibeDocumentError) -> BridgeError {
    MetadataError::InvalidVibeDocument {
        source: Box::new(source),
    }
    .into()
}

#[cfg(test)]
mod tests;
