use std::collections::BTreeMap;

use super::{ImageMetadata, MetadataOrigin, MetadataWarning, exif, stealth};
use crate::{BridgeError, ImageDecodePolicy, InvalidRequest, RequestField};

pub(super) fn parse_png(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<ImageMetadata, BridgeError> {
    let mut report = super::png::extract_png_text_chunks(bytes, policy, true)?;
    let pixels = report.pixels.take();
    let size = crate::ImageSize {
        width: report.usage.width,
        height: report.usage.height,
    };
    let mut metadata = super::interpret_png_text(report);
    if !metadata.text_chunks.is_empty() {
        return Ok(metadata);
    }
    let mut warnings = std::mem::take(&mut metadata.warnings);
    if let Some(metadata) = read_exif(bytes, policy, &mut warnings) {
        return Ok(metadata);
    }
    if let Some(pixels) = pixels {
        match stealth::read_png(&pixels, size, policy) {
            Ok(Some(text)) => {
                if let Some(metadata) =
                    json_carrier(&text, MetadataOrigin::StealthPngComp, &mut warnings)
                {
                    return Ok(metadata);
                }
            },
            Ok(None) => {},
            Err(error) => warnings.push(error),
        }
    }
    Ok(ImageMetadata {
        warnings,
        ..Default::default()
    })
}

pub(super) fn parse_image(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<ImageMetadata, BridgeError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return parse_png(bytes, policy);
    }
    let decoded = crate::image::decode_validated_image(bytes, policy).map_err(|source| {
        InvalidRequest::UndecodableImage {
            field: RequestField::MetadataImage,
            source,
        }
    })?;
    Ok(from_decoded(bytes, &decoded, policy))
}

/// Uses a media decoder's retained pixels, without decoding the image again.
pub fn from_decoded(
    bytes: &[u8],
    decoded: &crate::image::DecodedImage,
    policy: ImageDecodePolicy,
) -> ImageMetadata {
    let mut warnings = Vec::new();
    if let Some(metadata) = read_exif(bytes, policy, &mut warnings) {
        return metadata;
    }
    match stealth::read_pixels(&decoded.pixels, policy) {
        Ok(Some(text)) => {
            if let Some(metadata) =
                json_carrier(&text, MetadataOrigin::StealthPngComp, &mut warnings)
            {
                return metadata;
            }
        },
        Ok(None) => {},
        Err(error) => warnings.push(error),
    }
    ImageMetadata {
        warnings,
        ..Default::default()
    }
}

fn read_exif(
    bytes: &[u8],
    policy: ImageDecodePolicy,
    warnings: &mut Vec<MetadataWarning>,
) -> Option<ImageMetadata> {
    match exif::read(bytes, policy) {
        Ok(Some(text)) => json_carrier(&text, MetadataOrigin::ExifUserComment, warnings),
        Ok(None) => None,
        Err(error) => {
            warnings.push(error);
            None
        },
    }
}

fn json_carrier(
    text: &str,
    origin: MetadataOrigin,
    warnings: &mut Vec<MetadataWarning>,
) -> Option<ImageMetadata> {
    let value = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(serde_json::Value::Object(value)) => value,
        Ok(_) => {
            warnings.push(MetadataWarning::InvalidCarrier {
                origin,
                message: "metadata carrier must be a JSON object".to_owned(),
            });
            return None;
        },
        Err(error) => {
            warnings.push(MetadataWarning::InvalidCarrier {
                origin,
                message: error.to_string(),
            });
            return None;
        },
    };
    if value.is_empty() {
        return None;
    }
    let text_chunks: BTreeMap<String, Vec<String>> = value
        .into_iter()
        .filter_map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key, vec![value.replace('\0', "")]))
        })
        .collect();
    let mut metadata = super::interpret_text_entries(text_chunks, std::mem::take(warnings), origin);
    metadata.origin = Some(origin);
    Some(metadata)
}
