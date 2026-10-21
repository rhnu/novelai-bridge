use base64::{Engine, engine::general_purpose::STANDARD};
use image::imageops::FilterType;

use super::EncodedImage;
use crate::{
    BridgeError, ImageSize, InvalidRequest, RequestField,
    media_policy::{ImageDecodePolicy, ImageLimit, ImageResourceUsage},
};

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
#[derive(Clone, PartialEq, Eq)]
pub struct PreparedImage {
    bytes: Vec<u8>,
    size: ImageSize,
    usage: ImageResourceUsage,
}

impl PreparedImage {
    /// Validates a caller-produced PNG payload for use by a custom preprocessor.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the payload is not a decodable PNG.
    pub fn from_png(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        Self::from_png_with_policy(bytes, ImageDecodePolicy::default())
    }

    /// Validates a caller-produced PNG within explicit media budgets.
    ///
    /// # Errors
    /// Returns an invalid-request error for invalid PNG data or exceeded budgets.
    pub fn from_png_with_policy(
        bytes: impl Into<Vec<u8>>,
        policy: ImageDecodePolicy,
    ) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        let report = crate::metadata::png::extract_png_text_chunks(&bytes, policy, false).map_err(
            |source| InvalidRequest::UndecodableImage {
                field: RequestField::PreparedImage,
                source: Box::new(source),
            },
        )?;
        Ok(Self {
            bytes,
            size: ImageSize {
                width: report.usage.width,
                height: report.usage.height,
            },
            usage: report.usage,
        })
    }

    /// Encodes trusted pixels once, without re-decoding the PNG it produces.
    ///
    /// # Errors
    /// Returns a resource-budget or PNG-encoding error.
    pub fn from_pixels(pixels: &image::DynamicImage) -> Result<Self, BridgeError> {
        Self::from_pixels_with_policy(pixels, ImageDecodePolicy::default())
    }

    /// Encodes trusted pixels once within explicit media budgets.
    ///
    /// # Errors
    /// Returns a resource-budget or PNG-encoding error.
    pub fn from_pixels_with_policy(
        pixels: &image::DynamicImage,
        policy: ImageDecodePolicy,
    ) -> Result<Self, BridgeError> {
        let field = RequestField::PreparedImage;
        let size = ImageSize {
            width: pixels.width(),
            height: pixels.height(),
        };
        validate_size(field, size)?;
        policy
            .check_dimensions(size.width, size.height)
            .map_err(|source| invalid_policy(field, source))?;
        let decoded_bytes = pixels.as_bytes().len() as u64;
        policy
            .check(ImageLimit::DecodedBytes, decoded_bytes)
            .map_err(|source| invalid_policy(field, source))?;
        let bytes = super::codec::encode_png(pixels, field, policy)?;
        let usage = ImageResourceUsage {
            encoded_bytes: bytes.len() as u64,
            width: size.width,
            height: size.height,
            decoded_bytes,
            text_chunk_bytes: 0,
            total_text_bytes: 0,
        };
        Ok(Self { bytes, size, usage })
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
        field: RequestField,
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
        field: RequestField,
        image: &EncodedImage,
        operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError> {
        validate_inputs(field, image, operation)?;
        prepare_builtin(field, image, operation)
    }
}

pub fn prepare_image(
    preprocessor: Option<&dyn ImagePreprocessor>,
    field: RequestField,
    image: &EncodedImage,
    operation: ImagePreparation,
) -> Result<PreparedImage, BridgeError> {
    validate_inputs(field, image, operation)?;
    let Some(preprocessor) = preprocessor else {
        return prepare_builtin(field, image, operation);
    };
    let expected = operation.target_size();
    let prepared = preprocessor.prepare(field, image, operation)?;
    if prepared.size() != expected {
        return Err(crate::PreparationError::ImageSizeMismatch {
            field,
            expected,
            actual: prepared.size(),
        }
        .into());
    }
    prepared
        .usage
        .validate(ImageDecodePolicy::default())
        .map_err(|source| invalid_policy(field, source))?;
    Ok(prepared)
}

fn validate_inputs(
    field: RequestField,
    image: &EncodedImage,
    operation: ImagePreparation,
) -> Result<(), BridgeError> {
    let size = operation.target_size();
    validate_size(field, size)?;
    ImageDecodePolicy::default()
        .check_dimensions(size.width, size.height)
        .map_err(|source| invalid_policy(field, source))?;
    image.validate_policy(field, ImageDecodePolicy::default())
}

/// Executes the built-in transformation after the shared input boundary has validated it.
fn prepare_builtin(
    field: RequestField,
    image: &EncodedImage,
    operation: ImagePreparation,
) -> Result<PreparedImage, BridgeError> {
    let size = operation.target_size();
    let filter = match operation {
        ImagePreparation::Mask(_) => FilterType::Nearest,
        ImagePreparation::Source(_) | ImagePreparation::CharacterReference(_) => {
            FilterType::CatmullRom
        },
    };
    let bytes = super::fit::fit_decoded(field, image.pixels(), size, filter)?;
    let usage = ImageResourceUsage {
        encoded_bytes: bytes.len() as u64,
        width: size.width,
        height: size.height,
        decoded_bytes: u64::from(size.width) * u64::from(size.height) * 4,
        text_chunk_bytes: 0,
        total_text_bytes: 0,
    };
    Ok(PreparedImage { bytes, size, usage })
}

fn validate_size(field: RequestField, size: ImageSize) -> Result<(), BridgeError> {
    if size.width == 0 || size.height == 0 {
        return Err(InvalidRequest::ZeroImageDimension { field }.into());
    }
    Ok(())
}

fn invalid_policy(
    field: RequestField,
    source: crate::media_policy::ImagePolicyError,
) -> InvalidRequest {
    InvalidRequest::UndecodableImage {
        field,
        source: Box::new(source),
    }
}

impl std::fmt::Debug for PreparedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImage")
            .field("size", &self.size)
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
