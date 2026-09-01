use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{BridgeError, DataUrlError, InvalidRequest};

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

#[cfg(test)]
mod tests;
