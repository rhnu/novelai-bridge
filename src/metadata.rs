//! Image validation, metadata carriers and interpretation precedence.
mod exif;
mod novelai;
pub mod png;
mod read;
mod stealth;
mod types;
mod value;
mod vibe;
mod webui;

use png::first_chunk_value_ci;
pub use types::{
    CharacterMetadata, GeneratedImageMetadata, ImageMetadata, MetadataField, MetadataOrigin,
    MetadataWarning, VibeMetadataEntry, VibeMetadataSource,
};

use crate::{BridgeError, RequestField, media_policy::ImageDecodePolicy};
const KEY_SOURCE: &str = "Source";
const KEY_SOFTWARE: &str = "Software";

/// Parses PNG metadata from a base64 payload or image data URL.
///
/// # Errors
///
/// Returns [`BridgeError`] when the payload is empty, invalid base64/data URL input, or not a
/// valid PNG payload.
pub fn parse_png_metadata_from_payload(image: &str) -> Result<ImageMetadata, BridgeError> {
    let decoded = crate::payload::decode_image(
        RequestField::MetadataImage,
        image,
        ImageDecodePolicy::default(),
    )?;
    parse_png_metadata_from_bytes(decoded.as_slice())
}

/// Parses PNG metadata from raw PNG bytes.
///
/// # Errors
///
/// Returns [`BridgeError`] when `bytes` cannot be decoded as a PNG payload.
pub fn parse_png_metadata_from_bytes(bytes: &[u8]) -> Result<ImageMetadata, BridgeError> {
    parse_png_metadata_from_bytes_with_policy(bytes, ImageDecodePolicy::default())
}

/// Parses complete PNG bytes within explicit media resource budgets.
///
/// # Errors
/// Returns a metadata error for an invalid PNG or an exceeded resource budget.
pub fn parse_png_metadata_from_bytes_with_policy(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<ImageMetadata, BridgeError> {
    read::parse_png(bytes, policy)
}

/// Parses PNG, JPEG or WebP metadata after validating the complete supported media.
///
/// # Errors
/// Returns media validation or resource-budget failures. Invalid metadata is reported as warnings.
pub fn parse_image_metadata_from_bytes(bytes: &[u8]) -> Result<ImageMetadata, BridgeError> {
    parse_image_metadata_from_bytes_with_policy(bytes, ImageDecodePolicy::default())
}

/// Parses supported image metadata within explicit media and metadata budgets.
///
/// # Errors
/// Returns media validation or resource-budget failures.
pub fn parse_image_metadata_from_bytes_with_policy(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<ImageMetadata, BridgeError> {
    read::parse_image(bytes, policy)
}

pub use read::from_decoded as interpret_decoded_image;

pub fn interpret_png_text(extracted: png::ExtractedPngText) -> ImageMetadata {
    interpret_text_entries(
        extracted.text_chunks,
        extracted.warnings,
        MetadataOrigin::PngText,
    )
}

pub fn interpret_text_entries(
    text_chunks: std::collections::BTreeMap<String, Vec<String>>,
    warnings: Vec<MetadataWarning>,
    origin: MetadataOrigin,
) -> ImageMetadata {
    let mut parsed = ImageMetadata {
        origin: (!text_chunks.is_empty()).then_some(origin),
        source: first_chunk_value_ci(&text_chunks, KEY_SOURCE).map(str::to_owned),
        software: first_chunk_value_ci(&text_chunks, KEY_SOFTWARE).map(str::to_owned),
        text_chunks,
        warnings,
        ..Default::default()
    };

    vibe::parse_vibe_chunks(&mut parsed);
    novelai::parse_comment_payload(&mut parsed);
    webui::parse_parameters_payload(&mut parsed);

    if parsed.prompt.is_none() {
        parsed.prompt = first_chunk_value_ci(&parsed.text_chunks, "prompt")
            .or_else(|| first_chunk_value_ci(&parsed.text_chunks, "Description"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
    }
    if parsed.negative_prompt.is_none() {
        parsed.negative_prompt = first_chunk_value_ci(&parsed.text_chunks, "uc")
            .or_else(|| first_chunk_value_ci(&parsed.text_chunks, "negative_prompt"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
    }

    parsed
}

/// Inspects a generated image without promoting metadata problems to generation failures.
///
/// Text, Exif and stealth metadata carriers are inspected in that order within each image.
/// A supplied MIME type is only a hint; recognized media is always validated first.
#[must_use]
pub fn inspect_generated_image_metadata(
    bytes: &[u8],
    mime_type: Option<&str>,
) -> GeneratedImageMetadata {
    let supported = matches!(
        image::guess_format(bytes),
        Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)
    ) || mime_type.is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "image/png" | "image/jpeg" | "image/webp"
        )
    });
    if !supported {
        return GeneratedImageMetadata::UnsupportedFormat;
    }

    let parsed = if mime_type.is_some_and(|value| value.eq_ignore_ascii_case("image/png")) {
        parse_png_metadata_from_bytes(bytes)
    } else {
        parse_image_metadata_from_bytes(bytes)
    };
    match parsed {
        Ok(metadata) if metadata.is_empty() => GeneratedImageMetadata::NotPresent,
        Ok(metadata) => GeneratedImageMetadata::Parsed(Box::new(metadata)),
        Err(error) => GeneratedImageMetadata::Invalid {
            message: format_error_chain(&error),
        },
    }
}

pub fn format_error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        message.push_str(": ");
        message.push_str(&error.to_string());
        cause = error.source();
    }
    message
}

/// Extracts an official serialized vibe document from complete PNG bytes.
///
/// # Errors
/// Returns a metadata error when the PNG cannot be completely decoded.
pub fn extract_embedded_vibe_document_from_png_bytes(
    bytes: &[u8],
) -> Result<Option<String>, BridgeError> {
    vibe::extract_document(bytes)
}

#[cfg(test)]
pub mod test_support;
#[cfg(test)]
mod tests;
