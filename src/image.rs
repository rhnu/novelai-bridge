use std::{
    fmt,
    io::{Cursor, Read},
    sync::Arc,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
    DynamicImage, ImageFormat, Rgba, RgbaImage,
    imageops::{self, FilterType},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use crate::{
    BridgeError, DataUrlError, DecodeError, DecodeTarget, GeneratedImage, ImageSize,
    InvalidRequest, inspect_generated_image_metadata,
};

const REFERENCE_TARGETS: [(u32, u32); 3] = [(1024, 1536), (1536, 1024), (1472, 1472)];
const MAX_ARCHIVE_ENTRIES: usize = 64;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

/// Validated image input with raw bytes and decoded pixels cached together.
#[derive(Clone)]
pub struct EncodedImage {
    bytes: Arc<[u8]>,
    decoded: Arc<DynamicImage>,
}

impl EncodedImage {
    /// Validates image bytes and caches their decoded pixels.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the bytes are not a decodable image.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        let decoded = decode_input_image("image", &bytes)?;
        Ok(Self {
            bytes: bytes.into(),
            decoded: Arc::new(decoded),
        })
    }

    /// Decodes a base64 image or image data URL, validates it, and caches its pixels.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error for malformed base64, data URLs or image bytes.
    pub fn from_base64(value: &str) -> Result<Self, BridgeError> {
        Self::from_bytes(decode_base64("image", value)?)
    }

    /// Original validated bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Canonical base64 representation used by wire payloads.
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(&self.bytes)
    }

    /// Decoded dimensions.
    #[must_use]
    pub fn size(&self) -> ImageSize {
        ImageSize {
            width: self.decoded.width(),
            height: self.decoded.height(),
        }
    }

    fn decoded(&self) -> &DynamicImage {
        &self.decoded
    }
}

impl fmt::Debug for EncodedImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedImage")
            .field("size", &self.size())
            .field("bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
impl PartialEq for EncodedImage {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}
impl Eq for EncodedImage {}
impl Serialize for EncodedImage {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base64())
    }
}
impl<'de> Deserialize<'de> for EncodedImage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_base64(&value).map_err(D::Error::custom)
    }
}

/// Opaque vibe encoding bytes, distinct from an image payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VibeEncoding(Arc<[u8]>);

impl VibeEncoding {
    /// Creates a non-empty opaque encoding.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error when the encoding is empty.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self, BridgeError> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(InvalidRequest::EmptyField {
                field: "vibe_encoding".to_owned(),
            }
            .into());
        }
        Ok(Self(bytes.into()))
    }
    /// Decodes canonical or whitespace-padded base64.
    ///
    /// # Errors
    ///
    /// Returns an invalid-request error for empty or malformed base64.
    pub fn from_base64(value: &str) -> Result<Self, BridgeError> {
        Self::from_bytes(decode_plain_base64("vibe_encoding", value)?)
    }
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(&self.0)
    }
}
impl Serialize for VibeEncoding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_base64())
    }
}
impl<'de> Deserialize<'de> for VibeEncoding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::from_base64(&value).map_err(D::Error::custom)
    }
}

/// Semantic image preparation operation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImagePreparation {
    /// Fit an image-to-image source to this generation canvas.
    Source(ImageSize),
    /// Fit an inpaint mask to this generation canvas.
    Mask(ImageSize),
    /// Fit a director reference to the closest supported reference canvas.
    CharacterReference,
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
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Canonical base64 PNG representation used by wire payloads.
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(&self.bytes)
    }

    /// Prepared image dimensions.
    #[must_use]
    pub const fn size(&self) -> ImageSize {
        self.size
    }
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
        let (size, filter) = match operation {
            ImagePreparation::Source(size) => (size, FilterType::CatmullRom),
            ImagePreparation::Mask(size) => (size, FilterType::Nearest),
            ImagePreparation::CharacterReference => {
                (pick_reference_size(image.size()), FilterType::CatmullRom)
            }
        };
        let bytes = fit_decoded(field, image.decoded(), size, filter)?;
        Ok(PreparedImage { bytes, size })
    }
}

/// Strictly decodes a raw image or ZIP image response.
///
/// Any malformed supported-image candidate rejects the complete response. Metadata inspection is
/// best-effort and remains attached as state on an otherwise valid image.
///
/// # Errors
///
/// Returns a response decoding error for malformed archives, unsupported or invalid images,
/// extension/content mismatches, empty archives, and configured extraction-limit violations.
pub fn decode_generated_images(payload: Vec<u8>) -> Result<Vec<GeneratedImage>, BridgeError> {
    if payload.starts_with(b"PK") {
        return decode_zip(payload);
    }
    if payload.len() > usize::try_from(MAX_IMAGE_BYTES).unwrap_or(usize::MAX) {
        return Err(response_message("raw image response is too large"));
    }
    Ok(vec![validated_generated_image(payload, None)?])
}

#[cfg_attr(not(feature = "client"), allow(dead_code))]
pub fn decode_first_generated_image(payload: Vec<u8>) -> Result<GeneratedImage, BridgeError> {
    decode_generated_images(payload)?
        .into_iter()
        .next()
        .ok_or_else(|| response_message("image response contains no images"))
}

fn decode_zip(payload: Vec<u8>) -> Result<Vec<GeneratedImage>, BridgeError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(payload)).map_err(response_error)?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(response_message("image archive has too many entries"));
    }
    let mut total = 0_u64;
    let mut images = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(response_error)?;
        let name = file.name().to_owned();
        if image_extension(&name).is_none() {
            continue;
        }
        if file.size() > MAX_IMAGE_BYTES {
            return Err(response_message("image archive entry is too large"));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.by_ref()
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(response_error)?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_IMAGE_BYTES {
            return Err(response_message("image archive entry is too large"));
        }
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        if total > MAX_TOTAL_IMAGE_BYTES {
            return Err(response_message("image archive is too large"));
        }
        images.push(validated_generated_image(bytes, Some(&name))?);
    }
    if images.is_empty() {
        return Err(response_message(
            "image archive contains no supported images",
        ));
    }
    Ok(images)
}

fn validated_generated_image(
    bytes: Vec<u8>,
    name: Option<&str>,
) -> Result<GeneratedImage, BridgeError> {
    let format = image::guess_format(&bytes).map_err(response_error)?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP
    ) {
        return Err(response_message("response image format is unsupported"));
    }
    image::load_from_memory_with_format(&bytes, format).map_err(response_error)?;
    if let Some(extension) = name.and_then(image_extension) {
        let matches = matches!(
            (extension, format),
            ("png", ImageFormat::Png)
                | ("jpg" | "jpeg", ImageFormat::Jpeg)
                | ("webp", ImageFormat::WebP)
        );
        if !matches {
            return Err(response_message(
                "image archive extension does not match content",
            ));
        }
    }
    let mime_type = Some(
        match format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            _ => "image/webp",
        }
        .to_owned(),
    );
    let metadata = inspect_generated_image_metadata(&bytes, mime_type.as_deref());
    Ok(GeneratedImage {
        bytes,
        mime_type,
        metadata,
    })
}

fn response_error(source: impl std::error::Error + Send + Sync + 'static) -> BridgeError {
    DecodeError::new(DecodeTarget::ImageResponse, Box::new(source)).into()
}
fn response_message(message: &str) -> BridgeError {
    response_error(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

fn decode_input_image(field: &str, bytes: &[u8]) -> Result<DynamicImage, BridgeError> {
    image::load_from_memory(bytes).map_err(|source| {
        InvalidRequest::UndecodableImage {
            field: field.to_owned(),
            source: Box::new(source),
        }
        .into()
    })
}
fn decode_base64(field: &str, value: &str) -> Result<Vec<u8>, BridgeError> {
    let trimmed = value.trim();
    let payload = if trimmed.starts_with("data:") {
        let (meta, payload) =
            trimmed
                .split_once(',')
                .ok_or_else(|| InvalidRequest::InvalidDataUrl {
                    field: field.to_owned(),
                    reason: DataUrlError::MissingPayloadSeparator,
                })?;
        let meta = meta.to_ascii_lowercase();
        if !meta.starts_with("data:image/") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field.to_owned(),
                reason: DataUrlError::UnsupportedMediaType,
            }
            .into());
        }
        if !meta.contains(";base64") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field.to_owned(),
                reason: DataUrlError::MissingBase64Marker,
            }
            .into());
        }
        payload
    } else {
        trimmed
    };
    decode_plain_base64(field, payload)
}
fn decode_plain_base64(field: &str, value: &str) -> Result<Vec<u8>, BridgeError> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: field.to_owned(),
        }
        .into());
    }
    STANDARD.decode(compact).map_err(|source| {
        InvalidRequest::InvalidBase64 {
            field: field.to_owned(),
            source: Box::new(source),
        }
        .into()
    })
}

fn fit_decoded(
    field: &str,
    decoded: &DynamicImage,
    size: ImageSize,
    filter: FilterType,
) -> Result<Vec<u8>, BridgeError> {
    let source = decoded.to_rgba8();
    let (scaled_width, scaled_height) =
        fit_within(source.width(), source.height(), size.width, size.height);
    let resized = imageops::resize(&source, scaled_width, scaled_height, filter);
    let mut canvas = RgbaImage::from_pixel(size.width, size.height, Rgba([0, 0, 0, 255]));
    imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((size.width - scaled_width) / 2),
        i64::from((size.height - scaled_height) / 2),
    );
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|source| InvalidRequest::ImageEncodingFailed {
            field: field.to_owned(),
            source: Box::new(source),
        })?;
    Ok(output.into_inner())
}
fn fit_within(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32) {
    if u64::from(tw) * u64::from(sh) <= u64::from(th) * u64::from(sw) {
        (
            tw,
            u32::try_from(
                (u64::from(sh) * u64::from(tw) / u64::from(sw))
                    .max(1)
                    .min(u64::from(th)),
            )
            .unwrap_or(th),
        )
    } else {
        (
            u32::try_from(
                (u64::from(sw) * u64::from(th) / u64::from(sh))
                    .max(1)
                    .min(u64::from(tw)),
            )
            .unwrap_or(tw),
            th,
        )
    }
}
fn pick_reference_size(source: ImageSize) -> ImageSize {
    REFERENCE_TARGETS
        .into_iter()
        .min_by_key(|&(w, h)| {
            let (sw, sh) = fit_within(source.width, source.height, w, h);
            u64::from(w) * u64::from(h) - u64::from(sw) * u64::from(sh)
        })
        .map_or(source, |(width, height)| ImageSize { width, height })
}
fn image_extension(name: &str) -> Option<&str> {
    let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("png"),
        "jpg" => Some("jpg"),
        "jpeg" => Some("jpeg"),
        "webp" => Some("webp"),
        _ => None,
    }
}
