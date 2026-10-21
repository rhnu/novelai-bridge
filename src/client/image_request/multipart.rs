use std::collections::{HashMap, hash_map::Entry};

use serde_json::Value;

use crate::{
    BridgeError,
    image::request_body::{MediaBody, encode_projection},
};

pub(super) fn encode(value: &MediaBody) -> Result<(Vec<u8>, String), BridgeError> {
    let boundary = format!(
        "novelai-bridge-{}",
        super::hex(&super::random_bytes::<16>()?)
    );
    encode_with_boundary(value, &boundary)
}

fn encode_with_boundary(
    request: &MediaBody,
    boundary: &str,
) -> Result<(Vec<u8>, String), BridgeError> {
    let mut value = request.value.clone();
    let mut parts = HashMap::<&[u8], String>::new();
    let mut binaries = Vec::new();
    for media in request.media.iter().filter(|media| !media.omitted) {
        let Some(name) = part_name(&media.pointer) else {
            continue;
        };
        let name = match parts.entry(media.data.bytes()) {
            Entry::Occupied(existing) => existing.get().clone(),
            Entry::Vacant(entry) => {
                binaries.push((name.clone(), media.data.bytes()));
                entry.insert(name.clone());
                name
            },
        };
        *value
            .pointer_mut(&media.pointer)
            .expect("active media has a matching JSON field") = Value::String(name);
    }
    let json = encode_projection(
        &value,
        request
            .media
            .iter()
            .filter(|media| part_name(&media.pointer).is_none()),
    )?;
    let capacity = json.len()
        + binaries.iter().map(|(_, bytes)| bytes.len()).sum::<usize>()
        + (binaries.len() + 1) * (boundary.len() + 192)
        + boundary.len()
        + 6;
    let mut body = Vec::with_capacity(capacity);
    for (name, bytes) in binaries {
        append_part(&mut body, boundary, &name, "image/png", bytes);
    }
    append_part(&mut body, boundary, "request", "application/json", &json);
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, format!("multipart/form-data; boundary={boundary}")))
}

fn part_name(pointer: &str) -> Option<String> {
    match pointer {
        "/image" | "/parameters/image" => Some("image".to_owned()),
        "/mask" | "/parameters/mask" => Some("mask".to_owned()),
        "/parameters/reference_image" => Some("reference_image".to_owned()),
        _ => {
            for (field, name) in [
                (
                    "/parameters/reference_image_multiple_cached/",
                    "ref_multiple",
                ),
                (
                    "/parameters/director_reference_images_cached/",
                    "director_ref",
                ),
            ] {
                if let Some(index) = pointer
                    .strip_prefix(field)
                    .and_then(|tail| tail.strip_suffix("/data"))
                {
                    return Some(format!("{name}_{index}"));
                }
            }
            None
        },
    }
}

fn append_part(body: &mut Vec<u8>, boundary: &str, name: &str, mime: &str, bytes: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"blob\"\r\nContent-Type: {mime}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests;
