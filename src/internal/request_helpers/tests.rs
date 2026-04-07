use std::io::Cursor;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, Rgba, RgbaImage};

use super::*;

fn create_webp_bytes() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&[0, 1, 2, 3]);
    bytes
}

fn create_png_base64(width: u32, height: u32) -> String {
    let image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    STANDARD.encode(cursor.into_inner())
}

#[test]
fn normalize_image_payload_accepts_data_url() {
    let payload = normalize_image_payload("test.image", "data:image/png;base64,QUJDRA==").unwrap();
    assert_eq!(payload, "QUJDRA==");
}

#[test]
fn normalize_image_payload_rejects_non_image_data_url() {
    let error =
        normalize_image_payload("test.image", "data:text/plain;base64,QUJDRA==").unwrap_err();
    assert_eq!(error.code, "invalid_request");
}

#[test]
fn normalize_image_payload_rejects_empty_input() {
    let error = normalize_image_payload("test.image", "   ").unwrap_err();
    assert_eq!(error.code, "invalid_request");
    assert!(error.message.contains("cannot be empty"));
}

#[test]
fn extract_images_falls_back_to_raw_payload_mime() {
    let payload = create_webp_bytes();
    let images = extract_images(payload.clone(), 42);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bytes, payload);
    assert_eq!(images[0].mime_type.as_deref(), Some("image/webp"));
}

#[test]
fn preprocess_character_reference_image_selects_supported_target_sizes() {
    let cases = [
        (800, 1200, (1024, 1536)),
        (1200, 800, (1536, 1024)),
        (1000, 1000, (1472, 1472)),
    ];

    for (width, height, expected) in cases {
        let payload = create_png_base64(width, height);
        let encoded = preprocess_character_reference_image("reference.image", &payload).unwrap();
        let decoded = STANDARD.decode(encoded).unwrap();
        let image = image::load_from_memory(&decoded).unwrap();
        assert_eq!((image.width(), image.height()), expected);
    }
}
