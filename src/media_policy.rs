use thiserror::Error;

pub fn check_base64_budget(
    encoded_bytes: usize,
    padding: usize,
    policy: ImageDecodePolicy,
) -> Result<(), ImagePolicyError> {
    let decoded = (encoded_bytes as u64)
        .saturating_add(3)
        .saturating_div(4)
        .saturating_mul(3)
        .saturating_sub(padding as u64);
    policy.check(ImageLimit::EncodedBytes, decoded)
}

/// Resource budgets applied before allocating decoded media and metadata.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ImageDecodePolicy {
    /// Maximum original encoded bytes for one image, including base64-decoded input.
    pub max_encoded_bytes: u64,
    /// Maximum decoded width.
    pub max_width: u32,
    /// Maximum decoded height.
    pub max_height: u32,
    /// Maximum width multiplied by height.
    pub max_pixels: u64,
    /// Maximum size of one decoded pixel buffer, rather than a process-wide memory budget.
    pub max_decoded_bytes: u64,
    /// Per-chunk text budget, applied to raw inflation and retained UTF-8 text. Latin-1
    /// conversion can temporarily use up to twice this amount before UTF-8 accounting.
    pub max_text_chunk_bytes: usize,
    /// Maximum retained UTF-8 text across a PNG's chunks. Oversized compressed chunks
    /// produce recoverable text diagnostics without retaining their text.
    pub max_total_text_bytes: usize,
    /// Maximum number of ZIP entries, including ignored files.
    pub max_archive_entries: usize,
    /// Maximum returned images across raw, ZIP and JSON response containers.
    pub max_images: usize,
    /// Maximum cumulative returned image bytes; also caps the original ZIP input bytes.
    pub max_total_image_bytes: u64,
}

impl Default for ImageDecodePolicy {
    fn default() -> Self {
        Self {
            max_encoded_bytes: 64 * 1024 * 1024,
            max_width: 16_384,
            max_height: 16_384,
            max_pixels: 64 * 1024 * 1024,
            max_decoded_bytes: 256 * 1024 * 1024,
            max_text_chunk_bytes: 2 * 1024 * 1024,
            max_total_text_bytes: 8 * 1024 * 1024,
            max_archive_entries: 64,
            max_images: 1024,
            max_total_image_bytes: 256 * 1024 * 1024,
        }
    }
}

/// The resource whose configured budget was exceeded.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageLimit {
    EncodedBytes,
    Width,
    Height,
    Pixels,
    DecodedBytes,
    TextChunkBytes,
    TotalTextBytes,
    ArchiveEntries,
    Images,
    TotalImageBytes,
}

impl std::fmt::Display for ImageLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EncodedBytes => "encoded image bytes",
            Self::Width => "image width",
            Self::Height => "image height",
            Self::Pixels => "image pixels",
            Self::DecodedBytes => "decoded image bytes",
            Self::TextChunkBytes => "PNG text chunk bytes",
            Self::TotalTextBytes => "total PNG text bytes",
            Self::ArchiveEntries => "archive entries",
            Self::Images => "returned images",
            Self::TotalImageBytes => "total returned image bytes",
        })
    }
}

/// A typed media resource rejection, retained in the enclosing error's source chain.
#[derive(Copy, Clone, Debug, Error)]
#[error("{limit} ({actual}) exceeds the configured maximum ({maximum})")]
pub struct ImagePolicyError {
    pub limit: ImageLimit,
    pub actual: u64,
    pub maximum: u64,
}

impl ImageDecodePolicy {
    /// Checks one measured resource before allocating its storage.
    ///
    /// # Errors
    /// Returns the resource, observed amount and configured maximum when exceeded.
    pub fn check(self, limit: ImageLimit, actual: u64) -> Result<(), ImagePolicyError> {
        let maximum = match limit {
            ImageLimit::EncodedBytes => self.max_encoded_bytes,
            ImageLimit::Width => u64::from(self.max_width),
            ImageLimit::Height => u64::from(self.max_height),
            ImageLimit::Pixels => self.max_pixels,
            ImageLimit::DecodedBytes => self.max_decoded_bytes,
            ImageLimit::TextChunkBytes => self.max_text_chunk_bytes as u64,
            ImageLimit::TotalTextBytes => self.max_total_text_bytes as u64,
            ImageLimit::ArchiveEntries => self.max_archive_entries as u64,
            ImageLimit::Images => self.max_images as u64,
            ImageLimit::TotalImageBytes => self.max_total_image_bytes,
        };
        if actual > maximum {
            return Err(ImagePolicyError {
                limit,
                actual,
                maximum,
            });
        }
        Ok(())
    }

    /// Checks width, height and pixel count before allocating a canvas.
    ///
    /// # Errors
    /// Returns the first exceeded dimension or pixel-count budget.
    pub fn check_dimensions(self, width: u32, height: u32) -> Result<(), ImagePolicyError> {
        self.check(ImageLimit::Width, u64::from(width))?;
        self.check(ImageLimit::Height, u64::from(height))?;
        self.check(ImageLimit::Pixels, u64::from(width) * u64::from(height))
    }
}

/// Measurements retained by validated prepared payloads, avoiding a second pixel decode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ImageResourceUsage {
    pub encoded_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub decoded_bytes: u64,
    pub text_chunk_bytes: u64,
    pub total_text_bytes: u64,
}

impl ImageResourceUsage {
    pub fn validate(self, policy: ImageDecodePolicy) -> Result<(), ImagePolicyError> {
        policy.check(ImageLimit::EncodedBytes, self.encoded_bytes)?;
        policy.check_dimensions(self.width, self.height)?;
        policy.check(ImageLimit::DecodedBytes, self.decoded_bytes)?;
        policy.check(ImageLimit::TextChunkBytes, self.text_chunk_bytes)?;
        policy.check(ImageLimit::TotalTextBytes, self.total_text_bytes)
    }
}

#[cfg(test)]
mod tests;
