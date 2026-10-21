use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{BridgeError, DataUrlError, InvalidRequest};

#[cfg(test)]
mod tests;

pub fn decode_image(field: &str, value: &str) -> Result<Vec<u8>, BridgeError> {
    let trimmed = value.trim();
    let payload = if trimmed.starts_with("data:") {
        let (meta, payload) =
            trimmed
                .split_once(',')
                .ok_or_else(|| InvalidRequest::InvalidDataUrl {
                    field: field.to_owned(),
                    reason: DataUrlError::MissingPayloadSeparator,
                })?;
        let meta = meta.to_ascii_lowercase();
        if !meta.starts_with("data:image/") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field.to_owned(),
                reason: DataUrlError::UnsupportedMediaType,
            }
            .into());
        }
        if !meta.contains(";base64") {
            return Err(InvalidRequest::InvalidDataUrl {
                field: field.to_owned(),
                reason: DataUrlError::MissingBase64Marker,
            }
            .into());
        }
        payload
    } else {
        trimmed
    };
    decode_base64(field, payload)
}

pub fn decode_base64(field: &str, value: &str) -> Result<Vec<u8>, BridgeError> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField {
            field: field.to_owned(),
        }
        .into());
    }
    STANDARD.decode(compact).map_err(|source| {
        InvalidRequest::InvalidBase64 {
            field: field.to_owned(),
            source: Box::new(source),
        }
        .into()
    })
}
