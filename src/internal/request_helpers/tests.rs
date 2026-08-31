use std::io::Cursor;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, Rgba, RgbaImage};

use super::*;

fn create_webp_bytes() -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([255; 4])));
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::WebP)
        .unwrap();
    bytes.into_inner()
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
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidDataUrl {
            field,
            reason: DataUrlError::UnsupportedMediaType,
        }) if field == "test.image"
    ));
}

#[test]
fn normalize_image_payload_rejects_empty_input() {
    let error = normalize_image_payload("test.image", "   ").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { field })
            if field == "test.image"
    ));
}

#[test]
fn extract_images_falls_back_to_raw_payload_mime() {
    let payload = create_webp_bytes();
    let images = extract_images(payload.clone());
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bytes, payload);
    assert_eq!(images[0].mime_type.as_deref(), Some("image/webp"));
    assert!(matches!(
        images[0].metadata,
        crate::GeneratedImageMetadata::UnsupportedFormat
    ));
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
