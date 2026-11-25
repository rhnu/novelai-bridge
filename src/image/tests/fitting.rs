use image::{DynamicImage, Rgba, RgbaImage, imageops::FilterType};

use crate::{
    DefaultImagePreprocessor, EncodedImage, ImagePreparation, ImagePreprocessor, ImageSize,
    PreparedImage, RequestField,
};

#[test]
fn v5_precise_reference_selects_a_bucket_by_aspect_ratio() {
    for (source, expected) in [
        ((120, 120), (1024, 1024)),
        ((400, 600), (832, 1216)),
        ((600, 400), (1216, 832)),
        ((5000, 100), (4096, 256)),
    ] {
        let operation = ImagePreparation::bucket_character_reference(ImageSize {
            width: source.0,
            height: source.1,
        });
        let size = operation.target_size();
        assert_eq!((size.width, size.height), expected);
    }
}

#[test]
fn v5_precise_reference_in_an_existing_bucket_retains_original_media() {
    let mut jpeg = std::io::Cursor::new(Vec::new());
    DynamicImage::new_rgb8(1024, 1024)
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let source = EncodedImage::from_bytes(jpeg.into_inner()).unwrap();
    let prepared = DefaultImagePreprocessor
        .prepare(
            RequestField::Image,
            &source,
            ImagePreparation::bucket_character_reference(source.size()),
        )
        .unwrap();
    assert_eq!(prepared.format(), crate::ImageMediaFormat::Jpeg);
    assert_eq!(prepared.as_bytes(), source.as_bytes());
}

#[test]
fn v5_precise_reference_crops_the_center_without_black_padding() {
    let pixels = DynamicImage::ImageRgba8(RgbaImage::from_fn(4, 2, |x, _| {
        let red = if x == 0 || x == 3 { 0 } else { 200 };
        Rgba([red, 0, 0, 128])
    }));
    let result = prepare(
        &pixels,
        ImagePreparation::BucketCharacterReference(ImageSize {
            width: 2,
            height: 2,
        }),
    );
    assert_eq!(result.dimensions(), (2, 2));
    assert!(result.pixels().all(|pixel| pixel.0 == [200, 0, 0, 128]));
}

#[test]
fn v5_precise_reference_upscale_checks_intermediate_media_budget() {
    let source = super::image(1, 4096);
    let error = DefaultImagePreprocessor
        .prepare(
            RequestField::Image,
            &source,
            ImagePreparation::bucket_character_reference(source.size()),
        )
        .unwrap_err();
    let crate::BridgeError::InvalidRequest(crate::InvalidRequest::UndecodableImage {
        source, ..
    }) = error
    else {
        panic!("the oversized cover intermediate must be rejected as media");
    };
    let policy = source
        .downcast_ref::<crate::media_policy::ImagePolicyError>()
        .unwrap();
    assert_eq!(policy.limit, crate::media_policy::ImageLimit::DecodedBytes);
}

fn prepare(pixels: &DynamicImage, operation: ImagePreparation) -> RgbaImage {
    let source =
        EncodedImage::from_bytes(PreparedImage::from_pixels(pixels).unwrap().as_bytes()).unwrap();
    let prepared = DefaultImagePreprocessor
        .prepare(RequestField::Image, &source, operation)
        .unwrap();
    image::load_from_memory(prepared.as_bytes())
        .unwrap()
        .to_rgba8()
}

#[test]
fn identity_fitting_composites_translucent_and_transparent_pixels_onto_black() {
    let pixels = DynamicImage::ImageRgba8(
        RgbaImage::from_raw(3, 1, vec![255, 0, 0, 128, 12, 200, 99, 0, 20, 40, 60, 255]).unwrap(),
    );
    let size = ImageSize {
        width: 3,
        height: 1,
    };
    for operation in [ImagePreparation::Source(size), ImagePreparation::Mask(size)] {
        let result = prepare(&pixels, operation);
        assert_eq!(result.dimensions(), (3, 1));
        // image's f32 source-over blend truncates alpha 128 over opaque black to 254.
        // Preserve that existing overlay result rather than forcing mathematically opaque output.
        assert_eq!(
            result.as_raw(),
            &[128, 0, 0, 254, 0, 0, 0, 255, 20, 40, 60, 255]
        );
    }
    assert_eq!(
        pixels.as_rgba8().unwrap().get_pixel(0, 0).0,
        [255, 0, 0, 128]
    );
}

#[test]
fn identity_fitting_preserves_opaque_rgba_and_converts_rgb_without_changing_channels() {
    for pixels in [
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(2, 1, vec![1, 23, 45, 255, 67, 89, 10, 255]).unwrap(),
        ),
        DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(2, 1, vec![1, 23, 45, 67, 89, 10]).unwrap(),
        ),
    ] {
        let result = prepare(
            &pixels,
            ImagePreparation::Source(ImageSize {
                width: 2,
                height: 1,
            }),
        );
        assert_eq!(result.as_raw(), &[1, 23, 45, 255, 67, 89, 10, 255]);
    }
}

#[test]
fn resized_full_canvas_still_composites_alpha_onto_black() {
    let pixels = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([0, 255, 0, 128])));
    let result = prepare(
        &pixels,
        ImagePreparation::Source(ImageSize {
            width: 2,
            height: 2,
        }),
    );
    assert_eq!(result.dimensions(), (2, 2));
    assert!(result.pixels().all(|pixel| pixel.0 == [0, 128, 0, 254]));
}

#[test]
fn odd_borders_keep_the_role_specific_center_offset_without_resizing() {
    let pixels = DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 1, Rgba([255, 0, 0, 128])));
    let size = ImageSize {
        width: 3,
        height: 4,
    };
    let normal =
        crate::image::fit::fit_decoded(RequestField::Image, &pixels, size, FilterType::Nearest)
            .unwrap();
    let raw_vibe = crate::image::fit::fit_raw_vibe(RequestField::Image, &pixels, size).unwrap();
    for (bytes, red_row) in [(normal, 1), (raw_vibe, 2)] {
        let result = image::load_from_memory(&bytes).unwrap().to_rgba8();
        for y in 0..4 {
            for x in 0..3 {
                assert_eq!(
                    result.get_pixel(x, y).0,
                    if y == red_row {
                        [128, 0, 0, 254]
                    } else {
                        [0, 0, 0, 255]
                    }
                );
            }
        }
    }
}

#[test]
fn raw_vibe_rounds_the_scaled_side_while_source_fitting_floors_it() {
    let pixels = DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba([255, 0, 0, 255])));
    let size = ImageSize {
        width: 4,
        height: 4,
    };
    let normal =
        crate::image::fit::fit_decoded(RequestField::Image, &pixels, size, FilterType::Nearest)
            .unwrap();
    let raw_vibe = crate::image::fit::fit_raw_vibe(RequestField::Image, &pixels, size).unwrap();
    for (bytes, red_rows) in [(normal, 1..3), (raw_vibe, 1..4)] {
        let result = image::load_from_memory(&bytes).unwrap().to_rgba8();
        for y in 0..4 {
            assert_eq!(
                result.get_pixel(0, y).0,
                if red_rows.contains(&y) {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 255]
                }
            );
        }
    }
}

#[test]
fn director_identity_size_preserves_alpha_and_transparent_rgb() {
    let pixels = DynamicImage::ImageRgba8(
        RgbaImage::from_raw(2, 1, vec![255, 0, 0, 128, 12, 200, 99, 0]).unwrap(),
    );
    let result = prepare(
        &pixels,
        ImagePreparation::Director(ImageSize {
            width: 2,
            height: 1,
        }),
    );
    assert_eq!(result, *pixels.as_rgba8().unwrap());
}

#[test]
fn director_resize_preserves_translucent_pixels_without_black_compositing() {
    let pixels = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128])));
    let result = prepare(
        &pixels,
        ImagePreparation::Director(ImageSize {
            width: 2,
            height: 3,
        }),
    );
    assert_eq!(result.dimensions(), (2, 3));
    assert!(result.pixels().all(|pixel| pixel.0 == [255, 0, 0, 128]));
}
