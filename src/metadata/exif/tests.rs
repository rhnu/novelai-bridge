use super::*;
use crate::metadata::test_support::{ascii_comment, jpeg, png, tiff, webp};

#[test]
fn user_comments_are_recovered_from_png_webp_and_jpeg_carriers() {
    let json = r#"{"Description":"source","Comment":"{\"seed\":123}"}"#;
    let raw = tiff(&ascii_comment(json));
    for bytes in [png(&[], Some(&raw), None), webp(&raw), jpeg(&raw)] {
        assert_eq!(
            read(&bytes, ImageDecodePolicy::default())
                .unwrap()
                .as_deref(),
            Some(json)
        );
    }
}

#[test]
fn unicode_byte_order_and_null_termination_are_explicit() {
    let text = "日本語";
    for little in [false, true] {
        let mut bytes = b"UNICODE\0".to_vec();
        bytes.extend_from_slice(if little { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
        for unit in text.encode_utf16().chain([0]) {
            bytes.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        assert_eq!(decode_comment(&bytes, 128).unwrap(), text);
    }
    let mut bytes = ascii_comment("plain");
    bytes.extend_from_slice(&[0, 0]);
    assert_eq!(decode_comment(&bytes, 128).unwrap(), "plain");
    assert!(decode_comment(b"UNICODE\0\0", 128).is_err());
    assert!(decode_comment(b"UNICODE\0\xd8\0", 128).is_err());
    assert!(decode_comment(b"ASCII\0\0\0\xff", 128).is_err());
}

#[test]
fn invalid_or_oversized_exif_is_a_carrier_diagnostic() {
    let raw = tiff(&ascii_comment("{}"));
    let bytes = webp(&raw);
    let policy = ImageDecodePolicy {
        max_text_chunk_bytes: 8,
        ..Default::default()
    };
    assert!(matches!(
        read(&bytes, policy),
        Err(MetadataWarning::InvalidCarrier {
            origin: MetadataOrigin::ExifUserComment,
            ..
        })
    ));
    assert!(read(&webp(b"not TIFF"), ImageDecodePolicy::default()).is_err());
    assert!(
        read(&png(&[], None, None), ImageDecodePolicy::default())
            .unwrap()
            .is_none()
    );
}
