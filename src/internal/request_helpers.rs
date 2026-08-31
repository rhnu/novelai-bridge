use std::{
    cmp::Ordering,
    io::{Cursor, Read},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{
    DynamicImage, ImageFormat as EncodedImageFormat, Rgba, RgbaImage,
    imageops::{self, FilterType},
};

use crate::{
    BridgeError, Character, CharacterPosition, CharacterReference, DataUrlError, GeneratedImage,
    ImageSize, InvalidRequest, ResolvedCharacterPrompt, inspect_generated_image_metadata,
    internal::wire::{
        CenterPoint, CharacterPrompt, ReferenceCaption, ReferenceConditionInput, V4CharacterCaption,
    },
};

const DIRECTOR_TARGET_SIZES: [(u32, u32); 3] = [(1024, 1536), (1536, 1024), (1472, 1472)];
const GRID_COORDS: [f32; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];
const GRID_CENTER: f32 = 0.5;
const GRID_TOLERANCE: f32 = 0.0001;

/// Snaps a coordinate onto the 5x5 grid the `NovelAI` web client sends for manual placement.
pub fn snap_coord_to_grid(value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    let mut best = GRID_COORDS[0];
    let mut best_distance = (best - clamped).abs();
    for candidate in GRID_COORDS.into_iter().skip(1) {
        let distance = (candidate - clamped).abs();
        if distance < best_distance {
            best = candidate;
            best_distance = distance;
        }
    }
    best
}

fn center_point(position: CharacterPosition, use_coords: bool) -> CenterPoint {
    if use_coords {
        CenterPoint {
            x: snap_coord_to_grid(position.x),
            y: snap_coord_to_grid(position.y),
        }
    } else {
        CenterPoint {
            x: GRID_CENTER,
            y: GRID_CENTER,
        }
    }
}

pub fn is_center_position(value: f32) -> bool {
    (value - GRID_CENTER).abs() <= GRID_TOLERANCE
}

/// Builds the V4 character captions from already-resolved prompt text.
///
/// Text comes exclusively from [`ResolvedCharacterPrompt`]; this helper only decides geometry and
/// which characters take part. Re-deriving text from [`Character`] here would reintroduce a second
/// prompt-assembly path.
pub fn build_v4_character_captions(
    characters: &[Character],
    resolved: &[ResolvedCharacterPrompt],
    negative: bool,
    use_coords: bool,
) -> Vec<V4CharacterCaption> {
    let mut out = Vec::new();
    for resolved in resolved {
        let character = &characters[resolved.index];

        out.push(V4CharacterCaption {
            centers: Some(vec![center_point(character.position, use_coords)]),
            char_caption: Some(if negative {
                resolved.negative_prompt.clone()
            } else {
                resolved.prompt.clone()
            }),
        });
    }

    out
}

/// Builds the `characterPrompts` wire entries from already-resolved prompt text.
///
/// Only enabled, resolved characters are sent.
pub fn convert_character_prompts(
    characters: &[Character],
    resolved: &[ResolvedCharacterPrompt],
    use_coords: bool,
) -> Vec<CharacterPrompt> {
    resolved
        .iter()
        .map(|resolved| CharacterPrompt {
            prompt: resolved.prompt.clone(),
            uc: resolved.negative_prompt.clone(),
            center: center_point(characters[resolved.index].position, use_coords),
            enabled: true,
        })
        .collect()
}

pub struct CharacterReferenceWireParts {
    pub images: Vec<String>,
    pub descriptions: Vec<ReferenceConditionInput>,
    pub information_extracted: Vec<f32>,
    pub secondary_strength_values: Vec<f32>,
    pub strength_values: Vec<f32>,
}

pub fn convert_character_references(
    refs: &[CharacterReference],
) -> Result<CharacterReferenceWireParts, BridgeError> {
    let mut images = Vec::with_capacity(refs.len());
    let mut descriptions = Vec::with_capacity(refs.len());
    let mut information_extracted = Vec::with_capacity(refs.len());
    let mut secondary_strength_values = Vec::with_capacity(refs.len());
    let mut strength_values = Vec::with_capacity(refs.len());

    for (idx, item) in refs.iter().enumerate() {
        images.push(preprocess_character_reference_image(
            &format!("character_references[{idx}].image"),
            &item.image,
        )?);
        descriptions.push(ReferenceConditionInput {
            caption: Some(ReferenceCaption {
                base_caption: item.reference_type.as_str().to_owned(),
                char_captions: Vec::new(),
            }),
            legacy_uc: Some(false),
        });
        information_extracted.push(1.0);
        secondary_strength_values.push(1.0 - item.fidelity);
        strength_values.push(item.strength);
    }

    Ok(CharacterReferenceWireParts {
        images,
        descriptions,
        information_extracted,
        secondary_strength_values,
        strength_values,
    })
}

pub fn preprocess_character_reference_image(
    field_name: &str,
    raw: &str,
) -> Result<String, BridgeError> {
    let decoded = decode_image_payload(field_name, raw)?;
    let (width, height) = image_dimensions(field_name, &decoded)?;
    let (target_width, target_height) = pick_director_target_size(width, height);
    let processed = fit_image_to_canvas(
        field_name,
        &decoded,
        target_width,
        target_height,
        FilterType::CatmullRom,
    )?;
    Ok(STANDARD.encode(processed))
}

/// Decodes a base64 payload or image data URL into raw image bytes.
pub fn decode_image_payload(field_name: &str, raw: &str) -> Result<Vec<u8>, BridgeError> {
    let normalized = normalize_image_payload(field_name, raw)?;
    STANDARD
        .decode(&normalized)
        .map_err(|source| InvalidRequest::InvalidBase64 {
            field: field_name.to_owned(),
            source,
        })
        .map_err(Into::into)
}

/// Scales an image to fit a canvas, letterboxing with opaque black, and returns PNG bytes.
///
/// `NovelAI` requires image-to-image sources, inpaint masks and director references to match the
/// canvas they are used with, so every caller funnels through this one routine.
pub fn fit_image_to_canvas(
    field_name: &str,
    bytes: &[u8],
    target_width: u32,
    target_height: u32,
    filter: FilterType,
) -> Result<Vec<u8>, BridgeError> {
    let decoded =
        image::load_from_memory(bytes).map_err(|source| InvalidRequest::UndecodableImage {
            field: field_name.to_owned(),
            source,
        })?;

    let source = decoded.to_rgba8();
    let source_width = source.width();
    let source_height = source.height();
    if source_width == 0 || source_height == 0 {
        return Err(InvalidRequest::ZeroImageDimension {
            field: field_name.to_owned(),
        }
        .into());
    }

    let (scaled_width, scaled_height) =
        fit_within(source_width, source_height, target_width, target_height);
    let resized = imageops::resize(&source, scaled_width, scaled_height, filter);

    let mut canvas = RgbaImage::from_pixel(target_width, target_height, Rgba([0, 0, 0, 255]));
    let left = i64::from((target_width - scaled_width) / 2);
    let top = i64::from((target_height - scaled_height) / 2);
    imageops::overlay(&mut canvas, &resized, left, top);

    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut output, EncodedImageFormat::Png)
        .map_err(|source| InvalidRequest::ImageEncodingFailed {
            field: field_name.to_owned(),
            source,
        })?;
    Ok(output.into_inner())
}

/// Fits an encoded payload to a canvas and returns it as base64 PNG.
pub fn fit_encoded_image_to_canvas(
    field_name: &str,
    raw: &str,
    target_width: u32,
    target_height: u32,
    filter: FilterType,
) -> Result<String, BridgeError> {
    let decoded = decode_image_payload(field_name, raw)?;
    let fitted = fit_image_to_canvas(field_name, &decoded, target_width, target_height, filter)?;
    Ok(STANDARD.encode(fitted))
}

/// Fits an image-to-image source to the requested canvas.
pub fn fit_source_image(
    field_name: &str,
    raw: &str,
    size: ImageSize,
) -> Result<String, BridgeError> {
    fit_encoded_image_to_canvas(
        field_name,
        raw,
        size.width,
        size.height,
        FilterType::CatmullRom,
    )
}

/// Fits an inpaint mask to the requested canvas.
///
/// Nearest-neighbour scaling keeps the mask binary; the canvas is a multiple of 64 pixels, so the
/// result is already aligned to the 8 pixel grid `NovelAI` expects.
pub fn fit_mask_image(field_name: &str, raw: &str, size: ImageSize) -> Result<String, BridgeError> {
    fit_encoded_image_to_canvas(
        field_name,
        raw,
        size.width,
        size.height,
        FilterType::Nearest,
    )
}

fn image_dimensions(field_name: &str, bytes: &[u8]) -> Result<(u32, u32), BridgeError> {
    let decoded =
        image::load_from_memory(bytes).map_err(|source| InvalidRequest::UndecodableImage {
            field: field_name.to_owned(),
            source,
        })?;
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return Err(InvalidRequest::ZeroImageDimension {
            field: field_name.to_owned(),
        }
        .into());
    }
    Ok((width, height))
}

fn pick_director_target_size(source_width: u32, source_height: u32) -> (u32, u32) {
    let mut best = DIRECTOR_TARGET_SIZES[0];
    let mut best_score = director_padding_score(source_width, source_height, best.0, best.1);
    for candidate in DIRECTOR_TARGET_SIZES.into_iter().skip(1) {
        let score = director_padding_score(source_width, source_height, candidate.0, candidate.1);
        if score.cmp(&best_score) == Ordering::Less {
            best = candidate;
            best_score = score;
        }
    }
    best
}

fn director_padding_score(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> u64 {
    let (scaled_width, scaled_height) =
        fit_within(source_width, source_height, target_width, target_height);
    let target_area = u64::from(target_width) * u64::from(target_height);
    let used_area = u64::from(scaled_width) * u64::from(scaled_height);
    target_area.saturating_sub(used_area)
}

fn fit_within(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32) {
    let width_limited = u64::from(target_width) * u64::from(source_height)
        <= u64::from(target_height) * u64::from(source_width);

    if width_limited {
        let scaled_height = (u64::from(source_height) * u64::from(target_width)
            / u64::from(source_width))
        .max(1)
        .min(u64::from(target_height));
        (
            target_width,
            u32::try_from(scaled_height).unwrap_or(target_height),
        )
    } else {
        let scaled_width = (u64::from(source_width) * u64::from(target_height)
            / u64::from(source_height))
        .max(1)
        .min(u64::from(target_width));
        (
            u32::try_from(scaled_width).unwrap_or(target_width),
            target_height,
        )
    }
}

pub fn normalize_image_payload(field_name: &str, raw: &str) -> Result<String, BridgeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: field_name.to_owned(),
        }
        .into());
    }

    let base64_payload = if trimmed.starts_with("data:") {
        let (meta, payload) =
            trimmed
                .split_once(',')
                .ok_or_else(|| InvalidRequest::InvalidDataUrl {
                    field: field_name.to_owned(),
                    reason: DataUrlError::MissingPayloadSeparator,
                })?;
        let lower_meta = meta.to_ascii_lowercase();
        if !lower_meta.starts_with("data:image/") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field_name.to_owned(),
                reason: DataUrlError::UnsupportedMediaType,
            }
            .into());
        }
        if !lower_meta.contains(";base64") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field_name.to_owned(),
                reason: DataUrlError::MissingBase64Marker,
            }
            .into());
        }
        payload
    } else {
        trimmed
    };

    let compact: String = base64_payload
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: field_name.to_owned(),
        }
        .into());
    }
    STANDARD
        .decode(&compact)
        .map_err(|source| InvalidRequest::InvalidBase64 {
            field: field_name.to_owned(),
            source,
        })?;
    Ok(compact)
}

pub fn is_likely_base64(value: &str) -> bool {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.len() <= 24 {
        return false;
    }
    compact
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=')
}

pub fn extract_images(payload: Vec<u8>) -> Vec<GeneratedImage> {
    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(payload.as_slice())) {
        let mut images = Vec::new();
        for idx in 0..archive.len() {
            let Ok(mut file) = archive.by_index(idx) else {
                continue;
            };

            let lower_name = file.name().to_ascii_lowercase();
            if !is_supported_image_name(&lower_name) {
                continue;
            }

            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_ok() {
                let mime_type = infer_mime_from_name(&lower_name);
                let metadata = inspect_generated_image_metadata(&data, mime_type.as_deref());
                images.push(GeneratedImage {
                    bytes: data,
                    mime_type,
                    metadata,
                });
            }
        }

        if !images.is_empty() {
            return images;
        }
    }

    let mime_type = infer_mime_from_bytes(&payload);
    let metadata = inspect_generated_image_metadata(&payload, mime_type.as_deref());
    vec![GeneratedImage {
        bytes: payload,
        mime_type,
        metadata,
    }]
}

pub fn extract_first_image(payload: Vec<u8>) -> GeneratedImage {
    if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(payload.as_slice())) {
        for idx in 0..archive.len() {
            let Ok(mut file) = archive.by_index(idx) else {
                continue;
            };

            let lower_name = file.name().to_ascii_lowercase();
            if !is_supported_image_name(&lower_name) {
                continue;
            }

            let mut data = Vec::new();
            if file.read_to_end(&mut data).is_ok() {
                let mime_type = infer_mime_from_name(&lower_name);
                let metadata = inspect_generated_image_metadata(&data, mime_type.as_deref());
                return GeneratedImage {
                    bytes: data,
                    mime_type,
                    metadata,
                };
            }
        }
    }

    let mime_type = infer_mime_from_bytes(&payload);
    let metadata = inspect_generated_image_metadata(&payload, mime_type.as_deref());
    GeneratedImage {
        bytes: payload,
        mime_type,
        metadata,
    }
}

fn is_supported_image_name(name: &str) -> bool {
    image_extension(name).is_some()
}

fn infer_mime_from_name(name: &str) -> Option<String> {
    let extension = image_extension(name)?;
    if extension.eq_ignore_ascii_case("png") {
        Some("image/png".to_owned())
    } else if extension.eq_ignore_ascii_case("webp") {
        Some("image/webp".to_owned())
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        Some("image/jpeg".to_owned())
    } else {
        None
    }
}

fn image_extension(name: &str) -> Option<&str> {
    name.rsplit_once('.').map(|(_, ext)| ext).filter(|ext| {
        ext.eq_ignore_ascii_case("png")
            || ext.eq_ignore_ascii_case("jpg")
            || ext.eq_ignore_ascii_case("jpeg")
            || ext.eq_ignore_ascii_case("webp")
    })
}

fn infer_mime_from_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png".to_owned())
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg".to_owned())
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp".to_owned())
    } else {
        None
    }
}

pub fn default_seed() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| {
        let seed = (d.as_millis() % 10_000_000_000) as i64;
        if seed == 0 { 1 } else { seed }
    })
}

#[cfg(test)]
mod tests;
