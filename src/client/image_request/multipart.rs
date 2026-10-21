use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

use crate::{BridgeError, EncodeError, EncodeTarget};

pub(super) fn encode(value: &Value) -> Result<(Vec<u8>, String), BridgeError> {
    let boundary = format!(
        "novelai-bridge-{}",
        super::hex(&super::random_bytes::<16>()?)
    );
    encode_with_boundary(value, &boundary)
}

fn encode_with_boundary(value: &Value, boundary: &str) -> Result<(Vec<u8>, String), BridgeError> {
    let mut value = value.clone();
    let mut parts: Vec<(String, String)> = Vec::new();
    for field in ["image", "mask"] {
        if let Some(value) = value.get_mut(field) {
            extract(value, field, &mut parts);
        }
    }
    if let Some(parameters) = value.get_mut("parameters") {
        for field in ["image", "mask", "reference_image"] {
            if let Some(value) = parameters.get_mut(field) {
                extract(value, field, &mut parts);
            }
        }
        for (field, prefix) in [
            ("reference_image_multiple_cached", "ref_multiple"),
            ("director_reference_images_cached", "director_ref"),
        ] {
            if let Some(entries) = parameters.get_mut(field).and_then(Value::as_array_mut) {
                for (index, entry) in entries.iter_mut().enumerate() {
                    if let Some(data) = entry.get_mut("data") {
                        extract(data, &format!("{prefix}_{index}"), &mut parts);
                    }
                }
            }
        }
    }
    let mut body = Vec::new();
    for (name, data) in parts {
        let bytes = STANDARD
            .decode(data)
            .map_err(|source| EncodeError::new(EncodeTarget::JsonRequest, Box::new(source)))?;
        append_part(&mut body, boundary, &name, "image/png", &bytes);
    }
    append_part(
        &mut body,
        boundary,
        "request",
        "application/json",
        &serde_json::to_vec(&value).map_err(super::encode_error)?,
    );
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, format!("multipart/form-data; boundary={boundary}")))
}

fn extract(value: &mut Value, name: &str, parts: &mut Vec<(String, String)>) {
    if let Some(data) = value.as_str() {
        let name = parts
            .iter()
            .find(|(_, existing)| existing == data)
            .map_or_else(|| name.to_owned(), |(name, _)| name.clone());
        if !parts.iter().any(|(existing, _)| existing == &name) {
            parts.push((name.clone(), data.to_owned()));
        }
        *value = Value::String(name);
    }
}

fn append_part(body: &mut Vec<u8>, boundary: &str, name: &str, mime: &str, bytes: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"blob\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests;
