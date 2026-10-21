use serde_json::{Map, Value};

pub(super) fn extract_non_empty_string(
    object: &Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

pub(super) fn extract_v4_character_prompts(
    object: &Map<String, Value>,
    key: &str,
) -> Vec<Option<String>> {
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
                .map(|value| {
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

pub(super) fn extract_first_f32(object: &Map<String, Value>, keys: &[&str]) -> Option<f32> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_to_f32))
}

pub(super) fn value_to_f32_array(value: &Value) -> Option<Vec<Option<f32>>> {
    let values = value.as_array()?;
    Some(values.iter().map(value_to_f32).collect())
}

pub(super) fn value_to_bool(value: &Value) -> Option<bool> {
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

pub(super) fn value_to_i64(value: &Value) -> Option<i64> {
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

pub(super) fn value_to_i32(value: &Value) -> Option<i32> {
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

pub(super) fn value_to_u32(value: &Value) -> Option<u32> {
    if let Some(raw) = value.as_u64() {
        return u32::try_from(raw).ok();
    }
    if let Some(raw) = value.as_i64()
        && raw >= 0
    {
        return u32::try_from(raw).ok();
    }
    value
        .as_str()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
}

pub(super) fn value_to_f32(value: &Value) -> Option<f32> {
    let number = if let Some(raw) = value.as_f64() {
        f64_to_f32(raw)
    } else {
        value
            .as_str()
            .and_then(|raw| raw.trim().parse::<f32>().ok())?
    };
    number.is_finite().then_some(number)
}

pub(super) fn numeric_diagnostic(
    value: &Value,
    field: super::MetadataField,
    warnings: &mut Vec<super::PngMetadataWarning>,
) {
    if !value.is_null() && value_to_f32(value).is_none() {
        warnings.push(super::PngMetadataWarning::InvalidNumericValue {
            field,
            message: "value is not a finite representable f32".to_owned(),
        });
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Metadata exposes f32 values while JSON numbers are decoded as f64"
)]
pub(super) const fn f64_to_f32(raw: f64) -> f32 { raw as f32 }

pub(super) fn has_non_empty_value(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
}

pub(super) fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

pub(super) fn parse_json_payload(value: &str) -> Option<Value> {
    parse_json_payload_result(value).ok().flatten()
}

pub(super) fn parse_json_payload_result(value: &str) -> Result<Option<Value>, serde_json::Error> {
    let text = value.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let json = serde_json::from_str::<Value>(text)?;
    Ok(match json {
        Value::Object(_) => Some(json),
        Value::String(inner) => {
            Some(serde_json::from_str::<Value>(inner.trim())?).filter(Value::is_object)
        },
        _ => None,
    })
}
