use std::borrow::Cow;

use base64::{Engine, engine::general_purpose::STANDARD};

use crate::{
    BridgeError, DataUrlError, InvalidRequest, RequestField,
    media_policy::{ImageDecodePolicy, ImagePolicyError, check_base64_budget},
};

#[cfg(test)]
mod tests;

pub fn decode_image(
    field: RequestField,
    value: &str,
    policy: ImageDecodePolicy,
) -> Result<Vec<u8>, BridgeError> {
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
    decode_base64_with_policy(field, payload, Some(policy))
}

pub fn decode_base64(field: RequestField, value: &str) -> Result<Vec<u8>, BridgeError> {
    decode_base64_with_policy(field, value, None)
}

fn decode_base64_with_policy(
    field: RequestField,
    value: &str,
    policy: Option<ImageDecodePolicy>,
) -> Result<Vec<u8>, BridgeError> {
    let compact = compact_base64(value, policy).map_err(|source| match field {
        RequestField::MetadataImage => crate::MetadataError::InvalidPngPayload {
            field,
            source: Box::new(source),
        }
        .into(),
        _ => BridgeError::from(InvalidRequest::UndecodableImage {
            field,
            source: Box::new(source),
        }),
    })?;
    if compact.is_empty() {
        return Err(InvalidRequest::EmptyField { field }.into());
    }
    STANDARD.decode(compact.as_bytes()).map_err(|source| {
        InvalidRequest::InvalidBase64 {
            field,
            source: Box::new(source),
        }
        .into()
    })
}

/// Borrows compact input; whitespace removal bounds each buffer extension before allocation.
fn compact_base64(
    value: &str,
    policy: Option<ImageDecodePolicy>,
) -> Result<Cow<'_, str>, ImagePolicyError> {
    let compact = match value.char_indices().find(|(_, ch)| ch.is_whitespace()) {
        None => Cow::Borrowed(value),
        Some((index, _)) => {
            let mut compact = String::new();
            for part in std::iter::once(&value[..index]).chain(value[index..].split_whitespace()) {
                if let Some(policy) = policy {
                    // A prefix can still acquire two padding bytes. Use its minimum decoded
                    // length until the complete payload supplies the exact padding count.
                    check_base64_budget(compact.len().saturating_add(part.len()), 2, policy)?;
                }
                compact.push_str(part);
            }
            Cow::Owned(compact)
        },
    };
    if let Some(policy) = policy {
        let padding = if compact.ends_with("==") {
            2
        } else {
            usize::from(compact.ends_with('='))
        };
        check_base64_budget(compact.len(), padding, policy)?;
    }
    Ok(compact)
}
