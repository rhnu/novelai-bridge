use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    BridgeError, Character, CharacterPosition, CharacterReference, DataUrlError, ImagePreparation,
    ImagePreprocessor, InvalidRequest, ResolvedCharacterPrompt,
    internal::wire::{
        CenterPoint, CharacterPrompt, ReferenceCaption, ReferenceConditionInput, V4CharacterCaption,
    },
};

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
    preprocessor: &dyn ImagePreprocessor,
) -> Result<CharacterReferenceWireParts, BridgeError> {
    let mut images = Vec::with_capacity(refs.len());
    let mut descriptions = Vec::with_capacity(refs.len());
    let mut information_extracted = Vec::with_capacity(refs.len());
    let mut secondary_strength_values = Vec::with_capacity(refs.len());
    let mut strength_values = Vec::with_capacity(refs.len());

    for (idx, item) in refs.iter().enumerate() {
        images.push(
            preprocessor
                .prepare(
                    &format!("character_references[{idx}].image"),
                    &item.image,
                    ImagePreparation::CharacterReference,
                )?
                .to_base64(),
        );
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

pub fn default_seed() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| {
        let seed = (d.as_millis() % 10_000_000_000) as i64;
        if seed == 0 { 1 } else { seed }
    })
}

#[cfg(test)]
mod tests;
