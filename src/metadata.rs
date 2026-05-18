use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value};

use crate::{
    BridgeError, InvalidRequest, MetadataError,
    internal::request_helpers::{is_likely_base64, normalize_image_payload},
};

const KEY_COMMENT: &str = "Comment";
const KEY_SOURCE: &str = "Source";
const KEY_SOFTWARE: &str = "Software";
const KEY_PARAMETERS: &str = "parameters";
const KEY_VIBE_ENCODING: &str = "NovelAI_Vibe_Encoding_Base64";
const KEY_NAIDATA: &str = "naidata";
const KEY_LEGACY_VIBE: &str = "naiv4vibe";
const OFFICIAL_VIBE_IDENTIFIER: &str = "novelai-vibe-transfer";
const OFFICIAL_VIBE_BUNDLE_IDENTIFIER: &str = "novelai-vibe-transfer-bundle";

/// Indicates where a vibe metadata entry was recovered from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VibeMetadataSource {
    /// Read from the `NovelAI_Vibe_Encoding_Base64` PNG text chunk.
    NovelAiVibeEncodingChunk,
    /// Read from the `naidata` PNG text chunk.
    NaiDataChunk,
    /// Read from the legacy `naiv4vibe` PNG text chunk.
    LegacyVibeChunk,
    /// Reconstructed from the JSON payload stored in the `Comment` chunk.
    CommentPayload,
}

/// A single vibe-related metadata entry recovered from a PNG payload.
#[derive(Clone, Debug, PartialEq)]
pub struct VibeMetadataEntry {
    /// The source that produced this entry.
    pub source: VibeMetadataSource,
    /// Optional display name if present in the metadata.
    pub display_name: Option<String>,
    /// Primary base64-encoded vibe payload when available.
    pub encoding: Option<String>,
    /// Additional model-specific encodings keyed by model identifier.
    pub model_encodings: BTreeMap<String, String>,
    /// Optional strength value associated with the vibe.
    pub strength: Option<f32>,
    /// Optional information-extracted value associated with the vibe.
    pub information_extracted: Option<f32>,
    /// Whether the metadata entry references an embedded image or thumbnail.
    pub has_image: bool,
}

/// Parsed metadata recovered from a PNG image payload.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PngMetadata {
    /// Prompt text when present.
    pub prompt: Option<String>,
    /// Negative prompt text when present.
    pub negative_prompt: Option<String>,
    /// Seed value when present.
    pub seed: Option<i64>,
    /// Sampler name when present.
    pub sampler: Option<String>,
    /// Step count when present.
    pub steps: Option<u32>,
    /// CFG scale when present.
    pub scale: Option<f32>,
    /// Image width when present.
    pub width: Option<u32>,
    /// Image height when present.
    pub height: Option<u32>,
    /// Model identifier when present.
    pub model: Option<String>,
    /// Noise schedule when present.
    pub noise_schedule: Option<String>,
    /// CFG rescale value when present.
    pub cfg_rescale: Option<f32>,
    /// UC preset identifier when present.
    pub uc_preset: Option<i32>,
    /// Quality toggle flag when present.
    pub quality_toggle: Option<bool>,
    /// Source text chunk value when present.
    pub source: Option<String>,
    /// Software text chunk value when present.
    pub software: Option<String>,
    /// Extracted V4 character prompts.
    pub character_prompts: Vec<String>,
    /// Extracted V4 negative character prompts.
    pub character_negative_prompts: Vec<String>,
    /// Recovered vibe metadata entries.
    pub vibe_entries: Vec<VibeMetadataEntry>,
    /// All text chunks grouped by key.
    pub text_chunks: BTreeMap<String, Vec<String>>,
    /// Parsed JSON payload from the `Comment` chunk when available.
    pub raw_comment_json: Option<Value>,
}

#[derive(Default)]
struct ParameterFallback {
    prompt: Option<String>,
    negative_prompt: Option<String>,
    seed: Option<i64>,
    sampler: Option<String>,
    steps: Option<u32>,
    scale: Option<f32>,
    width: Option<u32>,
    height: Option<u32>,
    model: Option<String>,
}

/// Parses PNG metadata from a base64 payload or image data URL.
pub fn parse_png_metadata_from_payload(image: &str) -> Result<PngMetadata, BridgeError> {
    let normalized = normalize_image_payload("metadata.image", image)?;
    let decoded =
        STANDARD
            .decode(normalized.as_bytes())
            .map_err(|source| InvalidRequest::InvalidBase64 {
                field: "metadata.image".to_owned(),
                source,
            })?;
    parse_png_metadata_from_bytes(decoded.as_slice())
}

/// Parses PNG metadata from raw PNG bytes.
pub fn parse_png_metadata_from_bytes(bytes: &[u8]) -> Result<PngMetadata, BridgeError> {
    let text_chunks = extract_png_text_chunks(bytes)?;
    let mut parsed = PngMetadata {
        source: first_chunk_value_ci(&text_chunks, KEY_SOURCE).map(str::to_owned),
        software: first_chunk_value_ci(&text_chunks, KEY_SOFTWARE).map(str::to_owned),
        text_chunks,
        ..Default::default()
    };

    parse_vibe_chunks(&mut parsed);
    parse_comment_payload(&mut parsed);
    parse_parameters_payload(&mut parsed);

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

    Ok(parsed)
}

/// Extracts the serialized embedded vibe document from raw PNG bytes when present.
pub fn extract_embedded_vibe_document_from_png_bytes(
    bytes: &[u8],
) -> Result<Option<String>, BridgeError> {
    let text_chunks = extract_png_text_chunks(bytes)?;
    for raw in chunk_values_ci(&text_chunks, KEY_NAIDATA) {
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

fn extract_png_text_chunks(bytes: &[u8]) -> Result<BTreeMap<String, Vec<String>>, BridgeError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = decoder
        .read_info()
        .map_err(|source| MetadataError::InvalidPngPayload {
            field: "metadata.image".to_owned(),
            source,
        })?;
    let info = reader.info();

    let mut chunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for chunk in &info.uncompressed_latin1_text {
        push_text_chunk(&mut chunks, chunk.keyword.as_str(), chunk.text.as_str());
    }
    for chunk in &info.compressed_latin1_text {
        if let Ok(text) = chunk.get_text() {
            push_text_chunk(&mut chunks, chunk.keyword.as_str(), text.as_str());
        }
    }
    for chunk in &info.utf8_text {
        if let Ok(text) = chunk.get_text() {
            push_text_chunk(&mut chunks, chunk.keyword.as_str(), text.as_str());
        }
    }
    Ok(chunks)
}

fn push_text_chunk(chunks: &mut BTreeMap<String, Vec<String>>, key: &str, value: &str) {
    let key = key.trim();
    if key.is_empty() {
        return;
    }
    chunks
        .entry(key.to_owned())
        .or_default()
        .push(value.to_owned());
}

fn first_chunk_value_ci<'a>(
    chunks: &'a BTreeMap<String, Vec<String>>,
    key: &str,
) -> Option<&'a str> {
    chunks
        .iter()
        .find(|(chunk_key, _)| chunk_key.eq_ignore_ascii_case(key))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

fn chunk_values_ci<'a>(
    chunks: &'a BTreeMap<String, Vec<String>>,
    key: &str,
) -> impl Iterator<Item = &'a str> {
    chunks
        .iter()
        .filter(move |(chunk_key, _)| chunk_key.eq_ignore_ascii_case(key))
        .flat_map(|(_, values)| values.iter().map(String::as_str))
}

fn parse_vibe_chunks(parsed: &mut PngMetadata) {
    for raw in chunk_values_ci(&parsed.text_chunks, KEY_VIBE_ENCODING) {
        let compact = compact_base64(raw);
        if compact.is_empty() || !is_likely_base64(compact.as_str()) {
            continue;
        }
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
                payload,
                VibeMetadataSource::NaiDataChunk,
                &mut parsed.vibe_entries,
            );
        }
    }
    for raw in chunk_values_ci(&parsed.text_chunks, KEY_LEGACY_VIBE) {
        if let Some(payload) = parse_json_or_base64_json(raw) {
            push_vibe_entries(
                payload,
                VibeMetadataSource::LegacyVibeChunk,
                &mut parsed.vibe_entries,
            );
        }
    }
}

fn parse_comment_payload(parsed: &mut PngMetadata) {
    let Some(comment) = first_chunk_value_ci(&parsed.text_chunks, KEY_COMMENT) else {
        return;
    };
    let Some(payload) = parse_json_payload(comment) else {
        return;
    };
    let Some(object) = payload.as_object() else {
        return;
    };

    parsed.raw_comment_json = Some(payload.clone());
    parsed.prompt = extract_non_empty_string(object, &["prompt"]).or(parsed.prompt.take());
    parsed.negative_prompt = extract_non_empty_string(object, &["uc", "negative_prompt"])
        .or(parsed.negative_prompt.take());
    parsed.seed = object
        .get("seed")
        .and_then(value_to_i64)
        .or(parsed.seed.take());
    parsed.sampler = extract_non_empty_string(object, &["sampler"]).or(parsed.sampler.take());
    parsed.steps = object
        .get("steps")
        .and_then(value_to_u32)
        .or(parsed.steps.take());
    parsed.scale = extract_first_f32(
        object,
        &[
            "scale",
            "cfg_scale",
            "cfg",
            "guidance",
            "prompt_guidance",
            "cfgScale",
        ],
    )
    .or(parsed.scale.take());
    parsed.width = object
        .get("width")
        .and_then(value_to_u32)
        .or(parsed.width.take());
    parsed.height = object
        .get("height")
        .and_then(value_to_u32)
        .or(parsed.height.take());
    parsed.model = extract_non_empty_string(object, &["model"]).or(parsed.model.take());
    parsed.noise_schedule =
        extract_non_empty_string(object, &["noise_schedule"]).or(parsed.noise_schedule.take());
    parsed.cfg_rescale = object
        .get("cfg_rescale")
        .and_then(value_to_f32)
        .or(parsed.cfg_rescale.take());
    parsed.uc_preset = object
        .get("uc_preset")
        .or_else(|| object.get("ucPreset"))
        .and_then(value_to_i32)
        .or(parsed.uc_preset.take());
    parsed.quality_toggle = object
        .get("quality_toggle")
        .or_else(|| object.get("qualityToggle"))
        .and_then(value_to_bool)
        .or(parsed.quality_toggle.take());

    if parsed.character_prompts.is_empty() {
        parsed.character_prompts = extract_v4_character_prompts(object, "v4_prompt");
    }
    if parsed.character_negative_prompts.is_empty() {
        parsed.character_negative_prompts =
            extract_v4_character_prompts(object, "v4_negative_prompt");
    }

    push_reference_vibe_entries(object, &mut parsed.vibe_entries);
}

fn parse_parameters_payload(parsed: &mut PngMetadata) {
    let Some(parameters) = first_chunk_value_ci(&parsed.text_chunks, KEY_PARAMETERS) else {
        return;
    };
    let fallback = parse_webui_parameters(parameters);

    parsed.prompt = parsed.prompt.take().or(fallback.prompt);
    parsed.negative_prompt = parsed.negative_prompt.take().or(fallback.negative_prompt);
    parsed.seed = parsed.seed.take().or(fallback.seed);
    parsed.sampler = parsed.sampler.take().or(fallback.sampler);
    parsed.steps = parsed.steps.take().or(fallback.steps);
    parsed.scale = parsed.scale.take().or(fallback.scale);
    parsed.width = parsed.width.take().or(fallback.width);
    parsed.height = parsed.height.take().or(fallback.height);
    parsed.model = parsed.model.take().or(fallback.model);
}

fn parse_webui_parameters(input: &str) -> ParameterFallback {
    let mut fallback = ParameterFallback::default();
    let input = input.trim();
    if input.is_empty() {
        return fallback;
    }

    let mut prompt_block = input;
    let mut options_line = "";
    if let Some(index) = input.find("\nSteps:") {
        prompt_block = input[..index].trim();
        options_line = input[index + 1..].trim();
    } else if input.starts_with("Steps:") {
        prompt_block = "";
        options_line = input;
    }

    if let Some(index) = prompt_block.find("\nNegative prompt:") {
        fallback.prompt = non_empty_string(&prompt_block[..index]);
        fallback.negative_prompt =
            non_empty_string(&prompt_block[index + "\nNegative prompt:".len()..]);
    } else if let Some(rest) = prompt_block.strip_prefix("Negative prompt:") {
        fallback.negative_prompt = non_empty_string(rest);
    } else {
        fallback.prompt = non_empty_string(prompt_block);
    }

    if options_line.is_empty()
        && let Some(line) = input
            .lines()
            .find(|line| line.trim_start().starts_with("Steps:"))
    {
        options_line = line.trim();
    }

    for segment in options_line.split(',') {
        let mut pair = segment.trim().splitn(2, ':');
        let Some(key) = pair.next() else {
            continue;
        };
        let Some(value) = pair.next() else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "steps" => fallback.steps = value.parse::<u32>().ok(),
            "sampler" => fallback.sampler = non_empty_string(value),
            "cfg scale" | "cfg_scale" | "cfg" => fallback.scale = value.parse::<f32>().ok(),
            "seed" => fallback.seed = value.parse::<i64>().ok(),
            "size" => {
                if let Some((width, height)) = parse_size(value) {
                    fallback.width = Some(width);
                    fallback.height = Some(height);
                }
            }
            "model" => fallback.model = non_empty_string(value),
            _ => {}
        }
    }

    fallback
}

fn parse_size(value: &str) -> Option<(u32, u32)> {
    let mut parts = value.split('x').map(str::trim);
    let width = parts.next()?.parse::<u32>().ok()?;
    let height = parts.next()?.parse::<u32>().ok()?;
    Some((width, height))
}

fn parse_json_or_base64_json(value: &str) -> Option<Value> {
    parse_json_payload(value).or_else(|| {
        let compact = compact_base64(value);
        if compact.is_empty() || !is_likely_base64(compact.as_str()) {
            return None;
        }
        STANDARD
            .decode(compact.as_bytes())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|decoded| parse_json_payload(decoded.as_str()))
    })
}

fn parse_json_payload(value: &str) -> Option<Value> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    let json = serde_json::from_str::<Value>(text).ok()?;
    match json {
        Value::Object(_) => Some(json),
        Value::String(inner) => serde_json::from_str::<Value>(inner.trim())
            .ok()
            .filter(Value::is_object),
        _ => None,
    }
}

fn push_vibe_entries(
    payload: Value,
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
    if let Some(entry) = parse_vibe_entry(&payload, source) {
        output.push(entry);
    }
}

fn push_reference_vibe_entries(payload: &Map<String, Value>, output: &mut Vec<VibeMetadataEntry>) {
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
                    entry.strength = strengths.get(index).copied();
                }
                if entry.information_extracted.is_none() {
                    entry.information_extracted = infos.get(index).copied();
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
            let compact = compact_base64(encoded);
            if compact.is_empty() || !is_likely_base64(compact.as_str()) {
                return None;
            }
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
            .map(|value| compact_base64(value.as_str()));

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
                .map(|value| compact_base64(value.as_str()))
                .filter(|value| !value.is_empty());
            if let Some(candidate) = candidate {
                model_encodings.insert(key.to_owned(), candidate);
            }
        }
        if encoding.is_none() {
            encoding = model_encodings
                .get("vibe")
                .cloned()
                .or_else(|| model_encodings.values().next().cloned());
        }
    }

    if let Some(current) = encoding.as_ref()
        && !is_likely_base64(current.as_str())
    {
        encoding = None;
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

fn extract_non_empty_string(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn extract_v4_character_prompts(object: &Map<String, Value>, key: &str) -> Vec<String> {
    object
        .get(key)
        .and_then(Value::as_object)
        .and_then(|item| item.get("caption"))
        .and_then(Value::as_object)
        .and_then(|item| item.get("char_captions"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    value
                        .as_object()
                        .and_then(|item| item.get("char_caption"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|entry| !entry.is_empty())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn extract_first_f32(object: &Map<String, Value>, keys: &[&str]) -> Option<f32> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_to_f32))
}

fn value_to_f32_array(value: &Value) -> Option<Vec<f32>> {
    let values = value.as_array()?;
    Some(values.iter().filter_map(value_to_f32).collect())
}

fn value_to_bool(value: &Value) -> Option<bool> {
    if let Some(raw) = value.as_bool() {
        return Some(raw);
    }
    value
        .as_str()
        .and_then(|raw| match raw.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        })
}

fn value_to_i64(value: &Value) -> Option<i64> {
    if let Some(raw) = value.as_i64() {
        return Some(raw);
    }
    if let Some(raw) = value.as_u64() {
        return i64::try_from(raw).ok();
    }
    value
        .as_str()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
}

fn value_to_i32(value: &Value) -> Option<i32> {
    if let Some(raw) = value.as_i64() {
        return i32::try_from(raw).ok();
    }
    if let Some(raw) = value.as_u64() {
        return i32::try_from(raw).ok();
    }
    value
        .as_str()
        .and_then(|raw| raw.trim().parse::<i32>().ok())
}

fn value_to_u32(value: &Value) -> Option<u32> {
    if let Some(raw) = value.as_u64() {
        return u32::try_from(raw).ok();
    }
    if let Some(raw) = value.as_i64()
        && raw >= 0
    {
        return u32::try_from(raw as u64).ok();
    }
    value
        .as_str()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

fn value_to_f32(value: &Value) -> Option<f32> {
    if let Some(raw) = value.as_f64() {
        return Some(raw as f32);
    }
    value
        .as_str()
        .and_then(|raw| raw.trim().parse::<f32>().ok())
}

fn has_non_empty_value(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn compact_base64(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}
