use std::io::Write;

use super::VibeDocument;

/// Local document budgets, independent of provider reference-count or image limits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct VibeDocumentLimits {
    pub max_document_bytes: usize,
    pub max_vibes: usize,
    /// Total model groups plus parameter-cache records across the document.
    pub max_cache_entries: usize,
    /// UTF-8 bytes in one original image, encoding, mask or thumbnail string.
    pub max_payload_bytes: usize,
    /// Cumulative UTF-8 bytes in original payload strings, before base64 decoding.
    pub max_total_payload_bytes: usize,
}

impl Default for VibeDocumentLimits {
    fn default() -> Self {
        Self {
            max_document_bytes: 256 * 1024 * 1024,
            max_vibes: 1024,
            max_cache_entries: 4096,
            max_payload_bytes: 128 * 1024 * 1024,
            max_total_payload_bytes: 256 * 1024 * 1024,
        }
    }
}

/// Document resource whose configured local budget was exceeded.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VibeDocumentResource {
    DocumentBytes,
    Vibes,
    CacheEntries,
    PayloadBytes,
    TotalPayloadBytes,
}

/// Local vibe-document failures; payload text is not included in display messages.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VibeDocumentError {
    #[error("invalid vibe document JSON")]
    Json(#[from] serde_json::Error),
    #[error("vibe document {resource:?} ({actual}) exceeds the configured maximum ({maximum})")]
    Limit {
        resource: VibeDocumentResource,
        actual: usize,
        maximum: usize,
    },
    #[error("unsupported vibe document identifier at entry {entry:?}")]
    UnsupportedIdentifier { entry: Option<usize> },
    #[error("unsupported vibe document version {version} at entry {entry:?}")]
    UnsupportedVersion { entry: Option<usize>, version: u32 },
    #[error("vibe entry {entry} has an invalid SHA-256 identifier")]
    InvalidId { entry: usize },
    #[error("vibe entry {entry} payload does not match its identifier")]
    HashMismatch { entry: usize },
    #[error("vibe entry {entry} has no image payload")]
    MissingImage { entry: usize },
    #[error("vibe entry {entry} has no encoding record")]
    MissingEncoding { entry: usize },
    #[error("vibe entry {entry} thumbnail is not an image base64 data URL")]
    InvalidThumbnail { entry: usize },
    #[error("vibe entry {entry} has a non-finite {field}")]
    NonFiniteNumber { entry: usize, field: &'static str },
    #[error("selected vibe cache record is not a valid encoding")]
    InvalidEncoding {
        #[source]
        source: Box<crate::BridgeError>,
    },
}

impl VibeDocumentLimits {
    pub(super) const fn check(
        self,
        resource: VibeDocumentResource,
        actual: usize,
    ) -> Result<(), VibeDocumentError> {
        let maximum = match resource {
            VibeDocumentResource::DocumentBytes => self.max_document_bytes,
            VibeDocumentResource::Vibes => self.max_vibes,
            VibeDocumentResource::CacheEntries => self.max_cache_entries,
            VibeDocumentResource::PayloadBytes => self.max_payload_bytes,
            VibeDocumentResource::TotalPayloadBytes => self.max_total_payload_bytes,
        };
        if actual > maximum {
            Err(VibeDocumentError::Limit {
                resource,
                actual,
                maximum,
            })
        } else {
            Ok(())
        }
    }
}

struct DocumentWriter {
    bytes: Vec<u8>,
    limits: VibeDocumentLimits,
    rejection: Option<VibeDocumentError>,
}

impl Write for DocumentWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let next = self.bytes.len().saturating_add(bytes.len());
        if let Err(error) = self.limits.check(VibeDocumentResource::DocumentBytes, next) {
            self.rejection = Some(error);
            return Err(std::io::Error::other(
                "vibe document output exceeds its configured budget",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

pub(super) fn encode(
    document: &VibeDocument,
    limits: VibeDocumentLimits,
) -> Result<Vec<u8>, VibeDocumentError> {
    let mut writer = DocumentWriter {
        bytes: Vec::new(),
        limits,
        rejection: None,
    };
    if let Err(error) = serde_json::to_writer_pretty(&mut writer, document) {
        return Err(writer.rejection.unwrap_or_else(|| error.into()));
    }
    Ok(writer.bytes)
}
