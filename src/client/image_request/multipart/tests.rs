use serde_json::json;

use super::*;

#[test]
fn multipart_moves_official_binary_fields_and_deduplicates_identical_data() {
    let original = json!({ "parameters": {
        "image": "AQID", "mask": "BAUG",
        "reference_image_multiple_cached": [{"cache_secret_key":"key-a", "data":"AQID"}],
        "director_reference_images_cached": [{"cache_secret_key":"key-b", "data":"BwgJ"}]
    }});
    let (body, content_type) = encode_with_boundary(&original, "test-boundary").unwrap();
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
    let (body, _) = encode_with_boundary(&original, "boundary").unwrap();
    let text = String::from_utf8(body).unwrap();
    assert_eq!(text.matches("Content-Disposition:").count(), 1);
    assert!(text.contains("\"reference_image_multiple\":[\"AQID\"]"));
}
