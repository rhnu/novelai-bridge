use std::io::Cursor;

use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(width, height)
        .write_to(&mut output, image::ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

#[test]
fn prepared_upscale_sends_only_the_current_protocol_fields_and_reuses_the_source() {
    let bytes = png(3, 2);
    let image = EncodedImage::from_bytes(bytes).unwrap();
    let original = image.to_base64();
    let prepared = UpscaleImageRequest::new(image).prepare().unwrap();
    assert_eq!(
        prepared.image_size(),
        ImageSize {
            width: 3,
            height: 2
        }
    );
    let body: serde_json::Value = serde_json::from_slice(&prepared.encode().unwrap()).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "image": original,
            "model": "nai-diffusion-5-curated",
            "declared_blur_sigma": 0.0,
        })
    );
    assert_eq!(prepared.encode().unwrap(), prepared.encode().unwrap());
    assert_eq!(prepared.estimated_cost(), Some(1));
    assert!(!format!("{prepared:?}").contains(&original));
}

#[test]
fn blur_declarations_serialize_as_the_documented_numbers_and_reject_other_values() {
    for (sigma, number) in [
        (UpscaleBlurSigma::None, "0.0"),
        (UpscaleBlurSigma::Sigma030, "0.3"),
        (UpscaleBlurSigma::Sigma035, "0.35"),
        (UpscaleBlurSigma::Sigma040, "0.4"),
        (UpscaleBlurSigma::Sigma045, "0.45"),
        (UpscaleBlurSigma::Sigma050, "0.5"),
    ] {
        assert_eq!(serde_json::to_string(&sigma).unwrap(), number);
        assert_eq!(
            serde_json::from_str::<UpscaleBlurSigma>(number).unwrap(),
            sigma
        );
    }
    for value in ["-0.1", "0.1", "1.0", "null", "\"0.3\""] {
        assert!(serde_json::from_str::<UpscaleBlurSigma>(value).is_err());
    }
}

#[test]
fn current_price_table_has_explicit_boundaries_and_no_fabricated_price_beyond_them() {
    for (pixels, expected) in [
        (1, Some(1)),
        (1_048_576, Some(1)),
        (1_048_577, Some(2)),
        (1_747_627, Some(2)),
        (1_747_628, Some(3)),
        (2_446_678, Some(3)),
        (2_446_679, Some(4)),
        (3_145_728, Some(4)),
        (3_145_729, None),
    ] {
        assert_eq!(
            estimate_upscale_cost(ImageSize {
                width: pixels,
                height: 1
            }),
            expected
        );
    }
    assert_eq!(
        estimate_upscale_cost(ImageSize {
            width: 0,
            height: 100
        }),
        None
    );
}

#[test]
fn preparation_allows_images_beyond_the_ui_limit_and_the_known_price_table() {
    let image = EncodedImage::from_bytes(png(1792, 1792)).unwrap();
    let prepared = UpscaleImageRequest::new(image).prepare().unwrap();
    assert_eq!(
        prepared.image_size(),
        ImageSize {
            width: 1792,
            height: 1792
        }
    );
    assert_eq!(prepared.estimated_cost(), None);
}

#[test]
fn preparation_rechecks_default_media_budgets_for_relaxed_input_constructors() {
    let policy = crate::ImageDecodePolicy {
        max_width: 17_000,
        ..crate::ImageDecodePolicy::default()
    };
    let image = EncodedImage::from_bytes_with_policy(png(17_000, 1), policy).unwrap();
    assert!(UpscaleImageRequest::new(image).prepare().is_err());
}
