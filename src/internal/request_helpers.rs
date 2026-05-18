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
    BridgeError, Character, CharacterPosition, CharacterReference, CharacterReferenceType,
    DataUrlError, GeneratedImage, InvalidRequest,
    internal::wire::{
        CenterPoint, CharacterPrompt, ReferenceCaption, ReferenceConditionInput, V4CharacterCaption,
    },
};

const DIRECTOR_TARGET_SIZES: [(u32, u32); 3] = [(1024, 1536), (1536, 1024), (1472, 1472)];
const GRID_COORDS: [f32; 5] = [0.1, 0.3, 0.5, 0.7, 0.9];
const GRID_CENTER: f32 = 0.5;
const GRID_TOLERANCE: f32 = 0.0001;

fn snap_coord_to_grid(value: f32) -> f32 {
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

pub(crate) fn is_center_position(value: f32) -> bool {
    (value - GRID_CENTER).abs() <= GRID_TOLERANCE
}

pub(crate) fn build_v4_character_captions(
    characters: Option<&[Character]>,
    negative: bool,
    use_coords: bool,
) -> Vec<V4CharacterCaption> {
    let mut out = Vec::new();
    for ch in characters.unwrap_or_default() {
        if !ch.enabled {
            continue;
        }

        out.push(V4CharacterCaption {
            centers: Some(vec![center_point(ch.position, use_coords)]),
            char_caption: Some(if negative {
                ch.negative_prompt.clone().unwrap_or_default()
            } else {
                ch.prompt.clone()
            }),
        });
    }

    out
}

pub(crate) fn convert_character_prompts(
    characters: &[Character],
    use_coords: bool,
) -> Vec<CharacterPrompt> {
    characters
        .iter()
        .map(|ch| CharacterPrompt {
            prompt: ch.prompt.clone(),
            uc: ch.negative_prompt.clone().unwrap_or_default(),
            center: center_point(ch.position, use_coords),
            enabled: ch.enabled,
        })
        .collect()
}

pub(crate) struct CharacterReferenceWireParts {
    pub(crate) images: Vec<String>,
    pub(crate) descriptions: Vec<ReferenceConditionInput>,
    pub(crate) information_extracted: Vec<f32>,
    pub(crate) secondary_strength_values: Vec<f32>,
    pub(crate) strength_values: Vec<f32>,
}

pub(crate) fn convert_character_references(
    refs: Option<&[CharacterReference]>,
) -> Result<Option<CharacterReferenceWireParts>, BridgeError> {
    let Some(refs) = refs else {
        return Ok(None);
    };

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
                base_caption: match item.reference_type {
                    CharacterReferenceType::Character => "character".to_owned(),
                    CharacterReferenceType::Style => "style".to_owned(),
                    CharacterReferenceType::CharacterAndStyle => "character&style".to_owned(),
                },
                char_captions: Vec::new(),
            }),
            legacy_uc: Some(false),
        });
        information_extracted.push(1.0);
        secondary_strength_values.push(1.0 - item.fidelity);
        strength_values.push(item.strength);
    }

    Ok(Some(CharacterReferenceWireParts {
        images,
        descriptions,
        information_extracted,
        secondary_strength_values,
        strength_values,
    }))
}

pub(crate) fn preprocess_character_reference_image(
    field_name: &str,
    raw: &str,
) -> Result<String, BridgeError> {
    let normalized = normalize_image_payload(field_name, raw)?;
    let decoded = STANDARD
        .decode(&normalized)
        .map_err(|source| InvalidRequest::InvalidBase64 {
            field: field_name.to_owned(),
            source,
        })?;

    let processed = preprocess_director_reference_bytes(field_name, &decoded)?;
    Ok(STANDARD.encode(processed))
}

fn preprocess_director_reference_bytes(
    field_name: &str,
    bytes: &[u8],
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

    let (target_width, target_height) = pick_director_target_size(source_width, source_height);
    let (scaled_width, scaled_height) =
        fit_within(source_width, source_height, target_width, target_height);
    let resized = imageops::resize(&source, scaled_width, scaled_height, FilterType::CatmullRom);

    let mut canvas = RgbaImage::from_pixel(target_width, target_height, Rgba([0, 0, 0, 255]));
    let left = ((target_width - scaled_width) / 2) as i64;
    let top = ((target_height - scaled_height) / 2) as i64;
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
    let target_area = (target_width as u64) * (target_height as u64);
    let used_area = (scaled_width as u64) * (scaled_height as u64);
    target_area.saturating_sub(used_area)
}

fn fit_within(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32) {
    let scale = f64::min(
        target_width as f64 / source_width as f64,
        target_height as f64 / source_height as f64,
    );
    let scaled_width = ((source_width as f64) * scale).floor().max(1.0) as u32;
    let scaled_height = ((source_height as f64) * scale).floor().max(1.0) as u32;
    (scaled_width, scaled_height)
}

pub(crate) fn normalize_image_payload(field_name: &str, raw: &str) -> Result<String, BridgeError> {
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

pub(crate) fn is_likely_base64(value: &str) -> bool {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.len() <= 24 {
        return false;
    }
    compact
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '/' || ch == '=')
}

pub(crate) fn extract_images(payload: Vec<u8>, seed: i64) -> Vec<GeneratedImage> {
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
                images.push(GeneratedImage {
                    bytes: data,
                    mime_type: infer_mime_from_name(&lower_name),
                    seed: Some(seed),
                });
            }
        }

        if !images.is_empty() {
            return images;
        }
    }

    let mime_type = infer_mime_from_bytes(&payload);
    vec![GeneratedImage {
        bytes: payload,
        mime_type,
        seed: Some(seed),
    }]
}

pub(crate) fn extract_first_image(payload: Vec<u8>) -> GeneratedImage {
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
                return GeneratedImage {
                    bytes: data,
                    mime_type: infer_mime_from_name(&lower_name),
                    seed: None,
                };
            }
        }
    }

    let mime_type = infer_mime_from_bytes(&payload);
    GeneratedImage {
        bytes: payload,
        mime_type,
        seed: None,
    }
}

fn is_supported_image_name(name: &str) -> bool {
    name.ends_with(".png")
        || name.ends_with(".jpg")
        || name.ends_with(".jpeg")
        || name.ends_with(".webp")
}

fn infer_mime_from_name(name: &str) -> Option<String> {
    if name.ends_with(".png") {
        Some("image/png".to_owned())
    } else if name.ends_with(".webp") {
        Some("image/webp".to_owned())
    } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
        Some("image/jpeg".to_owned())
    } else {
        None
    }
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

pub(crate) fn default_seed() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| {
            let seed = (d.as_millis() % 10_000_000_000) as i64;
            if seed == 0 { 1 } else { seed }
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests;
