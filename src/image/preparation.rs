use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, imageops::FilterType};

use super::EncodedImage;
use crate::{BridgeError, ImageSize, InvalidRequest};

/// Semantic image preparation operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImagePreparation {
    /// Fit an image-to-image source to this generation canvas.
    Source(ImageSize),
    /// Fit an inpaint mask to this generation canvas.
    Mask(ImageSize),
    /// Fit a director reference to the closest supported reference canvas.
    CharacterReference(ImageSize),
}

impl ImagePreparation {
    /// Selects the supported reference canvas that wastes the least area.
    #[must_use]
    pub fn character_reference(source_size: ImageSize) -> Self {
        if source_size.width == 0 || source_size.height == 0 {
            Self::CharacterReference(source_size)
        } else {
            Self::CharacterReference(super::fit::pick_reference_size(source_size))
        }
    }

    /// Exact canvas the prepared payload must have.
    #[must_use]
    pub const fn target_size(self) -> ImageSize {
        match self {
            Self::Source(size) | Self::Mask(size) | Self::CharacterReference(size) => size,
        }
    }
}

/// Prepared PNG payload returned by an [`ImagePreprocessor`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedImage {
    bytes: Vec<u8>,
    size: ImageSize,
}

impl PreparedImage {
    /// Validates a caller-produced PNG payload for use by a custom preprocessor.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the payload is not a decodable PNG.
    pub fn from_png(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        let decoded =
            image::load_from_memory_with_format(&bytes, ImageFormat::Png).map_err(|source| {
                InvalidRequest::UndecodableImage {
                    field: "prepared_image".to_owned(),
                    source: Box::new(source),
                }
            })?;
        Ok(Self {
            bytes,
            size: ImageSize {
                width: decoded.width(),
                height: decoded.height(),
            },
        })
    }

    /// Validated PNG bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] { &self.bytes }

    /// Canonical base64 PNG representation used by wire payloads.
    #[must_use]
    pub fn to_base64(&self) -> String { STANDARD.encode(&self.bytes) }

    /// Prepared image dimensions.
    #[must_use]
    pub const fn size(&self) -> ImageSize { self.size }
}

/// Replaceable pixel transformation boundary. Inputs have already been validated and decoded.
pub trait ImagePreprocessor: Send + Sync {
    /// Produces the PNG payload required by one request role.
    ///
    /// # Errors
    ///
    /// Returns an image-preparation error without producing a partial payload.
    fn prepare(
        &self,
        field: &str,
        image: &EncodedImage,
        operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError>;
}

/// Default `NovelAI` canvas and reference fitting behavior.
#[derive(Copy, Clone, Debug, Default)]
pub struct DefaultImagePreprocessor;

impl ImagePreprocessor for DefaultImagePreprocessor {
    fn prepare(
        &self,
        field: &str,
        image: &EncodedImage,
        operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError> {
        let size = operation.target_size();
        validate_size(field, size)?;
        let filter = match operation {
            ImagePreparation::Mask(_) => FilterType::Nearest,
            ImagePreparation::Source(_) | ImagePreparation::CharacterReference(_) => {
                FilterType::CatmullRom
            },
        };
        let bytes = super::fit::fit_decoded(field, image.pixels(), size, filter)?;
        Ok(PreparedImage { bytes, size })
    }
}

#[cfg(any(feature = "generation", test))]
pub fn prepare_image(
    preprocessor: &dyn ImagePreprocessor,
    field: &str,
    image: &EncodedImage,
    operation: ImagePreparation,
) -> Result<PreparedImage, BridgeError> {
    let expected = operation.target_size();
    validate_size(field, expected)?;
    let prepared = preprocessor.prepare(field, image, operation)?;
    if prepared.size() != expected {
        return Err(InvalidRequest::PreparedImageSizeMismatch {
            field: field.to_owned(),
            expected,
            actual: prepared.size(),
        }
        .into());
    }
    Ok(prepared)
}

fn validate_size(field: &str, size: ImageSize) -> Result<(), BridgeError> {
    if size.width == 0 || size.height == 0 {
        return Err(InvalidRequest::ZeroImageDimension {
            field: field.to_owned(),
        }
        .into());
    }
    Ok(())
}
