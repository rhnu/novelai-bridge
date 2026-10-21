use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{BridgeError, DataUrlError, InvalidRequest, RequestField};

#[cfg(test)]
mod tests;

pub fn decode_image(field: RequestField, value: &str) -> Result<Vec<u8>, BridgeError> {
    let trimmed = value.trim();
    let payload = if trimmed.starts_with("data:") {
        let (meta, payload) =
            trimmed
                .split_once(',')
                .ok_or_else(|| InvalidRequest::InvalidDataUrl {
                    field,
                    reason: DataUrlError::MissingPayloadSeparator,
                })?;
        let meta = meta.to_ascii_lowercase();
        let mut parts = meta[5..].split(';');
        let media_type = parts.next().unwrap_or_default();
        if !media_type.starts_with("image/") || media_type.len() == "image/".len() {
            return Err(InvalidRequest::InvalidDataUrl {
                field,
                reason: DataUrlError::UnsupportedMediaType,
            }
            .into());
        }
        // The supported data URL form requires the exact, final base64 marker.
        let parameters: Vec<_> = parts.collect();
        if parameters.last() != Some(&"base64")
            || parameters[..parameters.len().saturating_sub(1)]
                .iter()
                .any(|part| !part.contains('='))
        {
            return Err(InvalidRequest::InvalidDataUrl {
                field,
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

pub fn decode_base64(field: RequestField, value: &str) -> Result<Vec<u8>, BridgeError> {
    let compact: String = value.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField { field }.into());
    }
    STANDARD.decode(compact).map_err(|source| {
        InvalidRequest::InvalidBase64 {
            field,
            source: Box::new(source),
        }
        .into()
    })
}
