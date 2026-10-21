use super::*;
use crate::metadata::{
    MetadataOrigin,
    test_support::{compressed, stealth_pixels},
};

fn image(
    width: u32,
    height: u32,
    color: png::ColorType,
    pixels: &[u8],
    interlaced: bool,
    text_after: bool,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut info = png::Info::with_size(width, height);
    info.color_type = color;
    info.bit_depth = png::BitDepth::Eight;
    info.interlaced = interlaced;
    let encoder = png::Encoder::with_info(&mut bytes, info).unwrap();
    let mut writer = encoder.write_header().unwrap();
    if interlaced {
        // Independent PNG fixture: unfiltered scanlines in the seven Adam7 pass orders.
        let channels = color.samples();
        let mut rows = Vec::new();
        for (dx, x0, dy, y0) in [
            (8, 0, 8, 0),
            (8, 4, 8, 0),
            (4, 0, 8, 4),
            (4, 2, 4, 0),
            (2, 0, 4, 2),
            (2, 1, 2, 0),
            (1, 0, 2, 1),
        ] {
            if x0 >= width as usize {
                continue;
            }
            for y in (y0..height as usize).step_by(dy) {
                rows.push(0);
                for x in (x0..width as usize).step_by(dx) {
                    let start = (y * width as usize + x) * channels;
                    rows.extend_from_slice(&pixels[start..start + channels]);
                }
            }
        }
        writer
            .write_chunk(png::chunk::IDAT, &compressed(&rows, 1))
            .unwrap();
    } else {
        writer.write_image_data(pixels).unwrap();
    }
    if text_after {
        writer
            .write_text_chunk(&png::text_metadata::TEXtChunk::new(
                "Description",
                "visible",
            ))
            .unwrap();
    }
    writer.finish().unwrap();
    bytes
}

#[test]
fn metadata_retains_no_full_pixels_and_uses_alpha_only_for_supported_fallbacks() {
    let with_text = crate::metadata::test_support::png(&[("Description", "visible")], None, None);
    let (report, alpha) = extract_png_metadata(&with_text, ImageDecodePolicy::default()).unwrap();
    assert!(report.pixels.is_none());
    assert!(alpha.is_none());
    for color in [png::ColorType::Rgb, png::ColorType::Grayscale] {
        let bytes = image(
            20,
            20,
            color,
            &vec![255; 400 * color.samples()],
            false,
            false,
        );
        let (report, alpha) = extract_png_metadata(&bytes, ImageDecodePolicy::default()).unwrap();
        assert!(report.pixels.is_none());
        assert!(alpha.is_none());
    }
}

#[test]
fn alpha_fallback_matches_all_pixels_for_gray_rgba_and_adam7() {
    let text = r#"{"Description":"hidden","Comment":"{\"seed\":42}"}"#;
    let (width, height, rgba) = stealth_pixels(&compressed(text.as_bytes(), 0));
    for interlaced in [false, true] {
        for color in [png::ColorType::Rgba, png::ColorType::GrayscaleAlpha] {
            let pixels = if color == png::ColorType::Rgba {
                rgba.clone()
            } else {
                rgba.as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|pixel| [pixel[0], pixel[3]])
                    .collect()
            };
            let bytes = image(width, height, color, &pixels, interlaced, false);
            let (report, alpha) =
                extract_png_metadata(&bytes, ImageDecodePolicy::default()).unwrap();
            assert!(report.pixels.is_none());
            assert_eq!(
                alpha
                    .unwrap()
                    .read(ImageDecodePolicy::default())
                    .unwrap()
                    .as_deref(),
                Some(text)
            );
            let metadata = crate::parse_png_metadata_from_bytes(&bytes).unwrap();
            assert_eq!(metadata.origin, Some(MetadataOrigin::StealthPngComp));
            assert_eq!(metadata.seed, Some(42));
            assert_eq!(metadata.prompt.as_deref(), Some("hidden"));
            let with_text = image(width, height, color, &pixels, interlaced, true);
            let metadata = crate::parse_png_metadata_from_bytes(&with_text).unwrap();
            assert_eq!(metadata.origin, Some(MetadataOrigin::PngText));
            assert_eq!(metadata.prompt.as_deref(), Some("visible"));
            assert_eq!(metadata.seed, None);
        }
    }
}

#[test]
fn expanded_palette_transparency_retains_the_stealth_carrier() {
    let text = r#"{"Comment":"{\"seed\":17}"}"#;
    let (width, height, rgba) = stealth_pixels(&compressed(text.as_bytes(), 0));
    let indices: Vec<_> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3] & 1)
        .collect();
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(&[255; 6]);
    encoder.set_trns(&[254, 255]);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&indices).unwrap();
    writer.finish().unwrap();
    let metadata = crate::parse_png_metadata_from_bytes(&bytes).unwrap();
    assert_eq!(metadata.origin, Some(MetadataOrigin::StealthPngComp));
    assert_eq!(metadata.seed, Some(17));
}

#[test]
fn sixteen_bit_png_alpha_is_not_interpreted_as_an_eight_bit_carrier() {
    let text = r#"{"Comment":"{\"seed\":17}"}"#;
    let (width, height, rgba) = stealth_pixels(&compressed(text.as_bytes(), 0));
    let pixels: Vec<_> = rgba
        .into_iter()
        .flat_map(|byte| (u16::from(byte) * 257).to_be_bytes())
        .collect();
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Sixteen);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    let (report, alpha) = extract_png_metadata(&bytes, ImageDecodePolicy::default()).unwrap();
    assert!(report.pixels.is_none());
    assert!(alpha.is_none());
    assert_eq!(
        crate::parse_png_metadata_from_bytes(&bytes).unwrap().seed,
        None
    );
}
