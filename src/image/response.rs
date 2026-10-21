use std::io::{Cursor, Read};

use super::{GeneratedImage, ImageMediaFormat};
use crate::{
    BridgeError, DecodeError, DecodeTarget, GeneratedImageMetadata,
    media_policy::{ImageDecodePolicy, ImageLimit},
};

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
    decode_generated_images_with_policy(payload, ImageDecodePolicy::default())
}

/// Decodes complete raw images or a bounded ZIP using explicit media budgets.
///
/// # Errors
/// Returns response errors for invalid images, archives or exceeded resource budgets.
pub fn decode_generated_images_with_policy(
    payload: Vec<u8>,
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    if payload.starts_with(b"PK") {
        policy
            .check(ImageLimit::TotalImageBytes, payload.len() as u64)
            .map_err(response_error)?;
        return decode_zip(payload, policy);
    }
    Ok(vec![validated_generated_image(payload, None, policy)?])
}

#[cfg(feature = "client")]
pub fn decode_first_generated_image(payload: Vec<u8>) -> Result<GeneratedImage, BridgeError> {
    decode_generated_images(payload)?
        .into_iter()
        .next()
        .ok_or_else(|| response_message("image response contains no images"))
}

fn decode_zip(
    payload: Vec<u8>,
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(payload)).map_err(response_error)?;
    policy
        .check(ImageLimit::ArchiveEntries, archive.len() as u64)
        .map_err(response_error)?;
    let mut total = 0_u64;
    let mut images = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(response_error)?;
        let name = file.name().map_err(response_error)?.into_owned();
        if ImageMediaFormat::from_extension(&name).is_none() {
            continue;
        }
        policy
            .check(ImageLimit::EncodedBytes, file.size())
            .map_err(response_error)?;
        policy
            .check(
                ImageLimit::TotalImageBytes,
                total.saturating_add(file.size()),
            )
            .map_err(response_error)?;
        let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.by_ref()
            .take(
                policy
                    .max_encoded_bytes
                    .min(policy.max_total_image_bytes.saturating_sub(total))
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .map_err(response_error)?;
        policy
            .check(ImageLimit::EncodedBytes, bytes.len() as u64)
            .map_err(response_error)?;
        total = total.saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        policy
            .check(ImageLimit::TotalImageBytes, total)
            .map_err(response_error)?;
        images.push(validated_generated_image(bytes, Some(&name), policy)?);
    }
    if images.is_empty() {
        return Err(response_message(
            "image archive contains no supported images",
        ));
    }
    Ok(images)
}

pub fn validated_generated_image(
    bytes: Vec<u8>,
    name: Option<&str>,
    policy: ImageDecodePolicy,
) -> Result<GeneratedImage, BridgeError> {
    policy
        .check(ImageLimit::EncodedBytes, bytes.len() as u64)
        .map_err(response_error)?;
    let format =
        ImageMediaFormat::from_image_format(image::guess_format(&bytes).map_err(response_error)?)
            .ok_or_else(|| response_message("response image format is unsupported"))?;

    if let Some(extension) = name.and_then(ImageMediaFormat::from_extension)
        && extension != format
    {
        return Err(response_message(
            "image archive extension does not match content",
        ));
    }
    let metadata = if format == ImageMediaFormat::Png {
        let report = crate::metadata::png::extract_png_text_chunks(&bytes, policy, false)
            .map_err(response_error)?;
        let parsed = crate::metadata::interpret_png_text(report);
        if parsed.is_empty() {
            GeneratedImageMetadata::NotPresent
        } else {
            GeneratedImageMetadata::Parsed(Box::new(parsed))
        }
    } else {
        super::codec::decode(&bytes, policy)
            .map_err(|source| DecodeError::new(DecodeTarget::ImageResponse, source))?;
        GeneratedImageMetadata::UnsupportedFormat
    };
    Ok(GeneratedImage {
        bytes,
        format,
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
