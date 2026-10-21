use std::collections::BTreeMap;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};
use crate::BridgeError;
use super::{PngMetadata, VibeMetadataEntry, VibeMetadataSource, png::{chunk_values_ci, extract_png_text_chunks}, value::*};
const KEY_VIBE_ENCODING: &str = "NovelAI_Vibe_Encoding_Base64";
const KEY_NAIDATA: &str = "naidata";
const KEY_LEGACY_VIBE: &str = "naiv4vibe";
const OFFICIAL_VIBE_IDENTIFIER: &str = "novelai-vibe-transfer";
const OFFICIAL_VIBE_BUNDLE_IDENTIFIER: &str = "novelai-vibe-transfer-bundle";

pub(super) fn parse_vibe_chunks(parsed: &mut PngMetadata) {
    for raw in chunk_values_ci(&parsed.text_chunks, KEY_VIBE_ENCODING) {
        let Some(compact) = validated_encoding(raw) else { continue; };
        parsed.vibe_entries.push(VibeMetadataEntry {
            source: VibeMetadataSource::NovelAiVibeEncodingChunk,
            display_name: None,
            encoding: Some(compact),
            model_encodings: BTreeMap::new(),
            strength: None,
            information_extracted: None,
            has_image: false,
        });
    }

    for raw in chunk_values_ci(&parsed.text_chunks, KEY_NAIDATA) {
        if let Some(payload) = parse_json_or_base64_json(raw) {
            push_vibe_entries(
                &payload,
                VibeMetadataSource::NaiDataChunk,
                &mut parsed.vibe_entries,
            );
        }
    }
    for raw in chunk_values_ci(&parsed.text_chunks, KEY_LEGACY_VIBE) {
        if let Some(payload) = parse_json_or_base64_json(raw) {
            push_vibe_entries(
                &payload,
                VibeMetadataSource::LegacyVibeChunk,
                &mut parsed.vibe_entries,
            );
        }
    }
}

fn parse_json_or_base64_json(value: &str) -> Option<Value> {
    parse_json_payload(value).or_else(|| {
        crate::payload::decode_base64("metadata.vibe", value)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|decoded| parse_json_payload(decoded.as_str()))
    })
}

fn push_vibe_entries(
    payload: &Value,
    source: VibeMetadataSource,
    output: &mut Vec<VibeMetadataEntry>,
) {
    if let Some(vibes) = payload
        .as_object()
        .and_then(|object| object.get("vibes"))
        .and_then(Value::as_array)
    {
        for vibe in vibes {
            if let Some(entry) = parse_vibe_entry(vibe, source) {
                output.push(entry);
            }
        }
        return;
    }
    if let Some(entry) = parse_vibe_entry(payload, source) {
        output.push(entry);
    }
}

pub(super) fn push_reference_vibe_entries(payload: &Map<String, Value>, output: &mut Vec<VibeMetadataEntry>) {
    if let Some(items) = payload
        .get("reference_image_multiple")
        .and_then(Value::as_array)
    {
        let strengths = payload
            .get("reference_strength_multiple")
            .and_then(value_to_f32_array)
            .unwrap_or_default();
        let infos = payload
            .get("reference_information_extracted_multiple")
            .and_then(value_to_f32_array)
            .unwrap_or_default();

        for (index, item) in items.iter().enumerate() {
            if let Some(mut entry) = parse_vibe_entry(item, VibeMetadataSource::CommentPayload) {
                if entry.strength.is_none() {
                    entry.strength = strengths.get(index).copied().flatten();
                }
                if entry.information_extracted.is_none() {
                    entry.information_extracted = infos.get(index).copied().flatten();
                }
                output.push(entry);
            }
        }
    }
    if let Some(item) = payload.get("reference_image")
        && let Some(entry) = parse_vibe_entry(item, VibeMetadataSource::CommentPayload)
    {
        output.push(entry);
    }
}

fn parse_vibe_entry(value: &Value, source: VibeMetadataSource) -> Option<VibeMetadataEntry> {
    match value {
        Value::String(encoded) => {
            let compact = validated_encoding(encoded)?;
            Some(VibeMetadataEntry {
                source,
                display_name: None,
                encoding: Some(compact),
                model_encodings: BTreeMap::new(),
                strength: None,
                information_extracted: None,
                has_image: false,
            })
        }
        Value::Object(object) => parse_vibe_map(object, source),
        _ => None,
    }
}

fn parse_vibe_map(
    object: &Map<String, Value>,
    source: VibeMetadataSource,
) -> Option<VibeMetadataEntry> {
    let mut model_encodings = BTreeMap::new();
    let mut encoding =
        extract_non_empty_string(object, &["vibe_encoding", "vibeEncoding", "encoding"])
            .and_then(|value| validated_encoding(&value));

    if let Some(encodings) = object.get("encodings").and_then(Value::as_object) {
        for (key, value) in encodings {
            let candidate = value
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    value
                        .as_object()
                        .and_then(|item| item.get("encoding"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .and_then(|value| validated_encoding(&value));
            if let Some(candidate) = candidate {
                model_encodings.insert(key.to_owned(), candidate);
            }
        }
        if encoding.is_none() {
            encoding = model_encodings.get("vibe").cloned();
        }
    }

    let import_info = object.get("importInfo").and_then(Value::as_object);
    let strength = object.get("strength").and_then(value_to_f32).or_else(|| {
        import_info
            .and_then(|info| info.get("strength"))
            .and_then(value_to_f32)
    });
    let information_extracted = object
        .get("information_extracted")
        .or_else(|| object.get("info_extracted"))
        .or_else(|| object.get("infoExtracted"))
        .and_then(value_to_f32)
        .or_else(|| {
            import_info
                .and_then(|info| info.get("information_extracted"))
                .and_then(value_to_f32)
        });
    let has_image =
        has_non_empty_value(object.get("image")) || has_non_empty_value(object.get("thumbnail"));
    let display_name = extract_non_empty_string(object, &["name", "displayName"]);

    if encoding.is_none() && model_encodings.is_empty() && !has_image {
        return None;
    }
    Some(VibeMetadataEntry {
        source,
        display_name,
        encoding,
        model_encodings,
        strength,
        information_extracted,
        has_image,
    })
}


fn validated_encoding(raw: &str) -> Option<String> {
    crate::payload::decode_base64("metadata.vibe", raw).ok().map(|bytes| STANDARD.encode(bytes))
}

/// Extracts the serialized embedded vibe document from raw PNG bytes when present.
///
/// # Errors
///
/// Returns [`BridgeError`] when `bytes` cannot be decoded as a PNG payload.
pub(super) fn extract_document(
    bytes: &[u8],
) -> Result<Option<String>, BridgeError> {
    let extracted = extract_png_text_chunks(bytes)?;
    for raw in chunk_values_ci(&extracted.text_chunks, KEY_NAIDATA) {
        let Some(payload) = parse_json_or_base64_json(raw) else {
            continue;
        };
        let Some(identifier) = payload
            .as_object()
            .and_then(|object| object.get("identifier"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if identifier != OFFICIAL_VIBE_IDENTIFIER && identifier != OFFICIAL_VIBE_BUNDLE_IDENTIFIER {
            continue;
        }
        let serialized = payload.to_string();
        return Ok(Some(serialized));
    }
    Ok(None)
}
