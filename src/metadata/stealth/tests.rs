use super::*;
use crate::metadata::test_support::{compressed, stealth_pixels};

fn pixels(compressed: &[u8]) -> (PngPixels, ImageSize) {
    let (width, height, bytes) = stealth_pixels(compressed);
    (
        PngPixels {
            bytes,
            color: png::ColorType::Rgba,
            depth: png::BitDepth::Eight,
        },
        ImageSize { width, height },
    )
}

#[test]
fn alpha_is_column_major_and_big_endian_for_all_confirmed_compression_forms() {
    let text = r#"{"Description":"日本語","Comment":"{\"seed\":42}"}"#;
    for codec in 0..3 {
        let (pixels, size) = pixels(&compressed(text.as_bytes(), codec));
        assert_eq!(
            read_png(&pixels, size, ImageDecodePolicy::default())
                .unwrap()
                .as_deref(),
            Some(text)
        );
    }
}

#[test]
fn zlib_and_raw_payloads_can_expand_across_multiple_output_buffers() {
    let text = serde_json::json!({"Description": "x".repeat(20 * 1024)}).to_string();
    for codec in [1, 2] {
        let encoded = compressed(text.as_bytes(), codec);
        let (pixels, size) = pixels(&encoded);
        let policy = ImageDecodePolicy {
            max_text_chunk_bytes: text.len(),
            ..Default::default()
        };
        assert_eq!(
            read_png(&pixels, size, policy).unwrap().as_deref(),
            Some(text.as_str())
        );
        assert!(decompress(&encoded, text.len() - 1).is_err());
    }
}

#[test]
fn raw_deflate_with_a_zlib_check_multiple_and_invalid_cinfo_stays_raw() {
    // Independent stored-block fixture: CINFO=8 is not a zlib header, even though
    // its first two bytes pass the modulo-31 check.
    let mut encoded = vec![0x88, 0x1C, 0x00, 0xE3, 0xFF];
    encoded.extend_from_slice(&[b' '; 26]);
    encoded.extend_from_slice(b"{}");
    encoded.extend_from_slice(&[1, 0, 0, 255, 255]);
    let decoded = decompress(&encoded, 28).unwrap();
    assert_eq!(
        decoded,
        [b"                          ".as_slice(), b"{}"].concat()
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&decoded).unwrap(),
        serde_json::json!({})
    );
}

#[test]
fn compressed_inflated_and_alpha_capacity_limits_are_checked() {
    let compressed = compressed(&vec![b'a'; 5000], 0);
    let (mut pixels, size) = pixels(&compressed);
    let policy = ImageDecodePolicy {
        max_text_chunk_bytes: 128,
        ..Default::default()
    };
    assert!(read_png(&pixels, size, policy).is_err());
    let policy = ImageDecodePolicy {
        max_text_chunk_bytes: 1,
        ..Default::default()
    };
    assert!(read_png(&pixels, size, policy).is_err());
    // Forge a huge bit count without constructing or allocating its claimed payload.
    for bit in 15 * 8..19 * 8 {
        let index =
            ((bit % size.height as usize) * size.width as usize + bit / size.height as usize) * 4
                + 3;
        pixels.bytes[index] |= 1;
    }
    assert!(read_png(&pixels, size, ImageDecodePolicy::default()).is_err());
}

#[test]
fn every_compression_form_rejects_truncation_and_trailing_bytes() {
    for codec in 0..3 {
        let encoded = compressed(b"a complete text payload", codec);
        assert!(decompress(&encoded[..encoded.len() - 1], 1024).is_err());
        let mut trailing = encoded;
        trailing.push(1);
        assert!(decompress(&trailing, 1024).is_err());
    }
}

#[test]
fn absent_and_unverified_sixteen_bit_carriers_are_not_guessed() {
    let (mut pixels, size) = pixels(&compressed(b"{}", 0));
    pixels.depth = png::BitDepth::Sixteen;
    assert!(
        read_png(&pixels, size, ImageDecodePolicy::default())
            .unwrap()
            .is_none()
    );
    pixels.depth = png::BitDepth::Eight;
    pixels.bytes.fill(255);
    assert!(
        read_png(&pixels, size, ImageDecodePolicy::default())
            .unwrap()
            .is_none()
    );
    pixels.color = png::ColorType::Rgb;
    assert!(
        read_png(&pixels, size, ImageDecodePolicy::default())
            .unwrap()
            .is_none()
    );
}
