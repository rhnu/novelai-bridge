use serde::{Deserialize, Serialize};

use crate::GeneratedImageMetadata;

/// Validated image payload returned by image response decoding.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GeneratedImage {
    /// Original image bytes.
    pub bytes: Vec<u8>,
    /// MIME type inferred from the response content.
    pub mime_type: Option<String>,
    /// Best-effort interpretation of this image's embedded metadata.
    pub metadata: GeneratedImageMetadata,
}

impl GeneratedImage {
    /// Seed recovered from this image, without a request-level fallback.
    #[must_use]
    pub fn seed(&self) -> Option<i64> { self.metadata.seed() }
}
