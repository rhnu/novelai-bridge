//! PNG validation, metadata formats and interpretation precedence.
mod novelai;
pub mod png;
mod types;
mod value;
mod vibe;
mod webui;

use png::{extract_png_text_chunks, first_chunk_value_ci};
pub use types::{
    CharacterMetadata, GeneratedImageMetadata, MetadataField, PngMetadata, PngMetadataWarning,
    VibeMetadataEntry, VibeMetadataSource,
};

use crate::{
    BridgeError, RequestField,
    media_policy::{ImageDecodePolicy, check_base64_budget},
};
const KEY_SOURCE: &str = "Source";
const KEY_SOFTWARE: &str = "Software";

/// Parses PNG metadata from a base64 payload or image data URL.
///
/// # Errors
///
/// Returns [`BridgeError`] when the payload is empty, invalid base64/data URL input, or not a
/// valid PNG payload.
pub fn parse_png_metadata_from_payload(image: &str) -> Result<PngMetadata, BridgeError> {
    check_base64_budget(image, ImageDecodePolicy::default()).map_err(|source| {
        crate::MetadataError::InvalidPngPayload {
            field: RequestField::MetadataImage,
            source: Box::new(source),
        }
    })?;
    let decoded = crate::payload::decode_image(RequestField::MetadataImage, image)?;
    parse_png_metadata_from_bytes(decoded.as_slice())
}

/// Parses PNG metadata from raw PNG bytes.
///
/// # Errors
///
/// Returns [`BridgeError`] when `bytes` cannot be decoded as a PNG payload.
pub fn parse_png_metadata_from_bytes(bytes: &[u8]) -> Result<PngMetadata, BridgeError> {
    parse_png_metadata_from_bytes_with_policy(bytes, ImageDecodePolicy::default())
}

/// Parses complete PNG bytes within explicit media resource budgets.
///
/// # Errors
/// Returns a metadata error for an invalid PNG or an exceeded resource budget.
pub fn parse_png_metadata_from_bytes_with_policy(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<PngMetadata, BridgeError> {
    Ok(interpret_png_text(extract_png_text_chunks(
        bytes, policy, false,
    )?))
}

pub fn interpret_png_text(extracted: png::ExtractedPngText) -> PngMetadata {
    let mut parsed = PngMetadata {
        source: first_chunk_value_ci(&extracted.text_chunks, KEY_SOURCE).map(str::to_owned),
        software: first_chunk_value_ci(&extracted.text_chunks, KEY_SOFTWARE).map(str::to_owned),
        text_chunks: extracted.text_chunks,
        warnings: extracted.warnings,
        ..Default::default()
    };

    vibe::parse_vibe_chunks(&mut parsed);
    novelai::parse_comment_payload(&mut parsed);
    webui::parse_parameters_payload(&mut parsed);

    if parsed.prompt.is_none() {
        parsed.prompt = first_chunk_value_ci(&parsed.text_chunks, "prompt")
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
/// PNG payloads are parsed with [`parse_png_metadata_from_bytes`]. Non-PNG payloads are reported
/// as [`GeneratedImageMetadata::UnsupportedFormat`]. A supplied MIME type is treated as a hint;
/// the PNG signature is also recognized.
#[must_use]
pub fn inspect_generated_image_metadata(
    bytes: &[u8],
    mime_type: Option<&str>,
) -> GeneratedImageMetadata {
    let is_png = bytes.starts_with(&[0x89, b'P', b'N', b'G'])
        || mime_type.is_some_and(|value| value.eq_ignore_ascii_case("image/png"));
    if !is_png {
        return GeneratedImageMetadata::UnsupportedFormat;
    }

    match parse_png_metadata_from_bytes(bytes) {
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
mod tests;
