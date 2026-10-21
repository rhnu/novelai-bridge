use serde::{Deserialize, Serialize};

use crate::GeneratedImageMetadata;

/// Validated image payload returned by image response decoding.
#[derive(Clone, Serialize, PartialEq)]
pub struct GeneratedImage {
    /// Original image bytes.
    pub(super) bytes: Vec<u8>,
    /// MIME type inferred from the response content.
    pub(super) format: ImageMediaFormat,
    /// Best-effort interpretation of this image's embedded metadata.
    pub(super) metadata: GeneratedImageMetadata,
}

/// Supported media formats with a single MIME/extension mapping.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImageMediaFormat {
    Png,
    Jpeg,
    WebP,
}

impl ImageMediaFormat {
    /// Canonical extension for files in this format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
        }
    }

    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::WebP => "image/webp",
        }
    }

    pub(super) fn from_extension(name: &str) -> Option<Self> {
        match name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::WebP),
            _ => None,
        }
    }

    pub(super) const fn from_image_format(format: image::ImageFormat) -> Option<Self> {
        match format {
            image::ImageFormat::Png => Some(Self::Png),
            image::ImageFormat::Jpeg => Some(Self::Jpeg),
            image::ImageFormat::WebP => Some(Self::WebP),
            _ => None,
        }
    }
}

impl GeneratedImage {
    #[must_use]
    pub fn bytes(&self) -> &[u8] { &self.bytes }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> { self.bytes }

    #[must_use]
    pub const fn format(&self) -> ImageMediaFormat { self.format }

    #[must_use]
    pub const fn mime_type(&self) -> &'static str { self.format.mime_type() }

    #[must_use]
    pub const fn metadata(&self) -> &GeneratedImageMetadata { &self.metadata }

    /// Seed recovered from this image, without a request-level fallback.
    #[must_use]
    pub fn seed(&self) -> Option<i64> { self.metadata.seed() }
}

impl std::fmt::Debug for GeneratedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GeneratedImage")
            .field("format", &self.format)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
