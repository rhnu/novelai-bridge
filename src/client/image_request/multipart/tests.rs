use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use super::*;
use crate::image::request_body::{Media, MediaBody as RequestBody, MediaSlot};

#[test]
fn multipart_moves_official_binary_fields_and_deduplicates_identical_data() {
    let original = json!({ "parameters": {
        "image": "AQID", "mask": "BAUG",
        "reference_image_multiple_cached": [{"cache_secret_key":"key-a", "data":"AQID"}],
        "director_reference_images_cached": [{"cache_secret_key":"key-b", "data":"BwgJ"}]
    }});
    let (body, content_type) =
        encode_with_boundary(&request_body(original.clone()), "test-boundary").unwrap();
    assert_eq!(content_type, "multipart/form-data; boundary=test-boundary");
    let text = String::from_utf8(body).unwrap();
    assert_eq!(text.matches("Content-Disposition:").count(), 4);
    assert!(text.contains("name=\"image\"; filename=\"blob\""));
    assert!(text.contains("name=\"mask\"; filename=\"blob\""));
    assert!(text.contains("name=\"director_ref_0\"; filename=\"blob\""));
    assert!(text.contains("name=\"request\"; filename=\"blob\""));
    assert!(!text.contains("name=\"ref_multiple_0\""));
    assert!(text.contains("\"data\":\"image\""));
    assert!(text.contains("\"data\":\"director_ref_0\""));
    assert!(text.contains("\r\n\r\n\u{1}\u{2}\u{3}\r\n"));
    assert!(text.ends_with("--test-boundary--\r\n"));
    assert_eq!(original["parameters"]["image"], "AQID");
}

#[test]
fn cacheless_reference_arrays_keep_their_base64_semantics() {
    let original = json!({"parameters":{"reference_image_multiple":["AQID"]}});
    let (body, _) = encode_with_boundary(&request_body(original), "boundary").unwrap();
    let text = String::from_utf8(body).unwrap();
    assert_eq!(text.matches("Content-Disposition:").count(), 1);
    assert!(text.contains("\"reference_image_multiple\":[\"AQID\"]"));
}

fn request_body(mut value: Value) -> RequestBody {
    let mut media = Vec::new();
    for pointer in [
        "/image",
        "/mask",
        "/parameters/image",
        "/parameters/mask",
        "/parameters/reference_image",
        "/parameters/reference_image_multiple/0",
        "/parameters/director_reference_images/0",
        "/parameters/reference_image_multiple_cached/0/data",
        "/parameters/director_reference_images_cached/0/data",
    ] {
        if let Some(field) = value.pointer_mut(pointer) {
            let bytes = STANDARD.decode(field.as_str().unwrap()).unwrap();
            *field = Value::String(String::new());
            media.push(MediaSlot {
                pointer: pointer.to_owned(),
                data: Media::Image(bytes.into()),
                omitted: false,
            });
        }
    }
    RequestBody::new(&value, media).unwrap()
}

#[test]
fn cached_multipart_omits_known_bytes_and_recovers_only_the_rejected_identity() {
    let original = json!({"parameters": {
        "image":"AQID", "mask":"BAUG", "reference_image_multiple":["AQID"], "director_reference_images":["AQID"]
    }});
    let mut cache = super::super::cache::CacheState::default();
    let secret = cache.secret().unwrap();
    let body = request_body(original.clone());
    let keys = super::super::cache::hash_media(&secret, &body);
    let first = cache.transform(body, keys);
    let (bytes, _) = encode_with_boundary(&first.body, "boundary").unwrap();
    let first_text = String::from_utf8(bytes).unwrap();
    assert_eq!(first_text.matches("Content-Disposition:").count(), 3);
    assert!(first_text.contains("\r\n\r\n\u{1}\u{2}\u{3}\r\n"));
    cache.register(first.registrations, 16);
    let body = request_body(original);
    let keys = super::super::cache::hash_media(&secret, &body);
    let mut cached = cache.transform(body, keys);
    let (bytes, _) = encode_with_boundary(&cached.body, "boundary").unwrap();
    let cached_text = String::from_utf8(bytes).unwrap();
    assert_eq!(cached_text.matches("Content-Disposition:").count(), 1);
    assert!(!cached_text.contains("AQID"));
    let rejected = cached.body.value["parameters"]["image_cache_secret_key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(cached.restore(&[rejected]));
    let (bytes, _) = encode_with_boundary(&cached.body, "boundary").unwrap();
    let restored = String::from_utf8(bytes).unwrap();
    assert_eq!(restored.matches("Content-Disposition:").count(), 2);
    assert!(restored.contains("\r\n\r\n\u{1}\u{2}\u{3}\r\n"));
    assert!(!restored.contains("name=\"mask\""));
    assert_eq!(restored.matches("\"data\":\"image\"").count(), 2);
}
