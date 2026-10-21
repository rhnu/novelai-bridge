use std::io::{Cursor, Read};

use image::ImageFormat;

use super::GeneratedImage;
use crate::{BridgeError, DecodeError, DecodeTarget, GeneratedImageMetadata};
const MAX_ARCHIVE_ENTRIES: usize = 64;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

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

#[cfg(feature = "client")]
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
    let metadata = if format == ImageFormat::Png {
        match crate::metadata::parse_png_metadata_from_bytes(&bytes) {
            Ok(parsed) if parsed.is_empty() => GeneratedImageMetadata::NotPresent,
            Ok(parsed) => GeneratedImageMetadata::Parsed(Box::new(parsed)),
            Err(error) => {
                image::load_from_memory_with_format(&bytes, format).map_err(response_error)?;
                GeneratedImageMetadata::Invalid {
                    message: error.to_string(),
                }
            },
        }
    } else {
        image::load_from_memory_with_format(&bytes, format).map_err(response_error)?;
        GeneratedImageMetadata::UnsupportedFormat
    };
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
