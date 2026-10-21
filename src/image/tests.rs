use std::{io::Write, sync::Mutex};

use super::output::GeneratedImageSource;
use crate::{
    BridgeError, DefaultImagePreprocessor, EncodedImage, GeneratedImageMetadata, ImagePreparation,
    ImagePreprocessor, ImageSize, PreparedImage, decode_generated_images,
};

fn image(width: u32, height: u32) -> EncodedImage {
    EncodedImage::from_bytes(create_png_bytes(width, height)).unwrap()
}

#[test]
fn encoded_image_validates_once_and_serializes_as_canonical_base64() {
    let bytes = create_png_bytes(3, 2);
    let encoded = EncodedImage::from_bytes(bytes.clone()).unwrap();
    assert_eq!(encoded.as_bytes(), bytes);
    assert_eq!(
        encoded.size(),
        ImageSize {
            width: 3,
            height: 2
        }
    );

    let json = serde_json::to_string(&encoded).unwrap();
    let roundtrip: EncodedImage = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip, encoded);
    assert!(EncodedImage::from_bytes(b"not an image".to_vec()).is_err());
}

#[test]
fn prepared_image_constructor_keeps_custom_implementations_validated() {
    let prepared = PreparedImage::from_png(create_png_bytes(5, 4)).unwrap();
    assert_eq!(
        prepared.size(),
        ImageSize {
            width: 5,
            height: 4
        }
    );
    assert!(PreparedImage::from_png(b"not png".to_vec()).is_err());
}

#[test]
fn response_decoder_accepts_valid_raw_image_and_rejects_arbitrary_bytes() {
    let png = create_png_bytes(2, 2);
    let images = decode_generated_images(png.clone()).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].bytes(), png);
    assert_eq!(images[0].mime_type(), "image/png");
    assert_eq!(images[0].source(), &GeneratedImageSource::Raw);
    assert!(decode_generated_images(b"not an image".to_vec()).is_err());
}

#[test]
fn zip_response_preserves_all_image_names_and_original_entry_indices() {
    let first = create_png_bytes(2, 2);
    let second = create_png_bytes(3, 1);
    let third = create_png_bytes(1, 4);
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    for (name, compression, bytes) in [
        (
            "readme.txt",
            zip::CompressionMethod::Stored,
            b"not an image entry".as_slice(),
        ),
        (
            "图像/first.PNG",
            zip::CompressionMethod::Deflated,
            first.as_slice(),
        ),
        (
            "second.png",
            zip::CompressionMethod::Stored,
            second.as_slice(),
        ),
        (
            "other/third.png",
            zip::CompressionMethod::Deflated,
            third.as_slice(),
        ),
    ] {
        let options = zip::write::SimpleFileOptions::default().compression_method(compression);
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
    let images = decode_generated_images(cursor.into_inner()).unwrap();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0].bytes(), first);
    assert_eq!(images[1].bytes(), second);
    assert_eq!(images[2].bytes(), third);
    for (image, (name, index)) in images.iter().zip([
        ("图像/first.PNG", 1),
        ("second.png", 2),
        ("other/third.png", 3),
    ]) {
        assert_eq!(
            image.source(),
            &GeneratedImageSource::Archive {
                name: name.to_owned(),
                index,
            }
        );
    }
}

#[test]
fn an_invalid_non_first_image_rejects_the_whole_archive() {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("good.png", options).unwrap();
    writer.write_all(&create_png_bytes(2, 2)).unwrap();
    writer.start_file("broken.png", options).unwrap();
    writer.write_all(b"not an image").unwrap();
    writer.finish().unwrap();
    assert!(decode_generated_images(cursor.into_inner()).is_err());
}

#[test]
fn archive_entry_limit_is_enforced_before_extraction() {
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(&mut cursor);
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..65 {
        writer
            .start_file(format!("ignored-{index}.txt"), options)
            .unwrap();
    }
    writer.finish().unwrap();
    assert!(decode_generated_images(cursor.into_inner()).is_err());
}

#[test]
fn metadata_problems_do_not_discard_a_valid_image() {
    let png = create_png_with_comment_text("not json");
    let image = decode_generated_images(png).unwrap().pop().unwrap();
    assert!(matches!(
        image.metadata(),
        GeneratedImageMetadata::Parsed(_)
    ));
}

#[derive(Default)]
struct CountingPreprocessor {
    operations: Mutex<Vec<ImagePreparation>>,
}

impl ImagePreprocessor for CountingPreprocessor {
    fn prepare(
        &self,
        field: crate::RequestField,
        image: &EncodedImage,
        operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError> {
        self.operations.lock().unwrap().push(operation);
        DefaultImagePreprocessor.prepare(field, image, operation)
    }
}

#[test]
fn custom_preprocessor_observes_each_semantic_input_once() {
    use crate::{
        CharacterReference, CharacterReferenceType, GenerateImageRequest, InpaintingInput, Model,
    };
    let inpainting = InpaintingInput::new(image(4, 3), image(4, 3), 0.5, 0.0);
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .inpainting(inpainting)
        .character_references(vec![CharacterReference {
            image: image(3, 4),
            reference_type: CharacterReferenceType::Character,
            fidelity: 0.5,
            strength: 0.5,
        }])
        .build();
    let preprocessor = CountingPreprocessor::default();
    request
        .normalize()
        .unwrap()
        .prepare_with(&preprocessor)
        .unwrap();
    assert_eq!(
        *preprocessor.operations.lock().unwrap(),
        vec![
            ImagePreparation::Source(ImageSize {
                width: 64,
                height: 64,
            }),
            ImagePreparation::Mask(ImageSize {
                width: 64,
                height: 64,
            }),
            ImagePreparation::character_reference(ImageSize {
                width: 3,
                height: 4
            }),
        ]
    );
}

fn create_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![255; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    bytes
}

fn create_png_with_comment_text(comment: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder
        .add_text_chunk("Comment".to_owned(), comment.to_owned())
        .unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[255; 16]).unwrap();
    writer.finish().unwrap();
    bytes
}

struct PixelCopyPreprocessor;

impl ImagePreprocessor for PixelCopyPreprocessor {
    fn prepare(
        &self,
        _field: crate::RequestField,
        image: &EncodedImage,
        _operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError> {
        PreparedImage::from_pixels(image.pixels())
    }
}

#[test]
fn custom_preprocessor_reuses_pixels_and_must_match_the_resolved_canvas() {
    let image = image(3, 2);
    let prepared = super::prepare_image(
        Some(&PixelCopyPreprocessor),
        crate::RequestField::Image,
        &image,
        ImagePreparation::Source(image.size()),
    )
    .unwrap();
    assert_eq!(prepared.size(), image.size());
    let expected = ImageSize {
        width: 8,
        height: 8,
    };
    assert!(
        matches!(super::prepare_image(Some(&PixelCopyPreprocessor), crate::RequestField::Image, &image, ImagePreparation::Mask(expected)), Err(BridgeError::Preparation(crate::PreparationError::ImageSizeMismatch { expected: actual_target, actual, .. })) if actual_target == expected && actual == image.size())
    );
}

#[test]
fn preprocessing_rejects_zero_canvas_before_calling_the_adapter() {
    let preprocessor = CountingPreprocessor::default();
    assert!(
        super::prepare_image(
            Some(&preprocessor),
            crate::RequestField::Image,
            &image(2, 2),
            ImagePreparation::Source(ImageSize {
                width: 0,
                height: 2
            })
        )
        .is_err()
    );
    assert!(preprocessor.operations.lock().unwrap().is_empty());
}

#[test]
fn builtin_entry_points_preserve_canvas_validation_and_prepared_pixels() {
    let source = image(2, 2);
    let field = crate::RequestField::Image;
    let target = ImageSize {
        width: 4,
        height: 3,
    };
    let operation = ImagePreparation::Source(target);
    let direct = DefaultImagePreprocessor
        .prepare(field, &source, operation)
        .unwrap();
    let internal = super::prepare_image(None, field, &source, operation).unwrap();
    assert_eq!(direct, internal);
    assert_eq!(direct.size(), target);
    assert_eq!(
        EncodedImage::from_bytes(direct.as_bytes()).unwrap().size(),
        target
    );

    for size in [
        ImageSize {
            width: 0,
            height: 2,
        },
        ImageSize {
            width: 2,
            height: 0,
        },
    ] {
        for error in [
            DefaultImagePreprocessor
                .prepare(field, &source, ImagePreparation::Mask(size))
                .unwrap_err(),
            super::prepare_image(None, field, &source, ImagePreparation::Mask(size)).unwrap_err(),
        ] {
            assert!(matches!(
                error,
                BridgeError::InvalidRequest(crate::InvalidRequest::ZeroImageDimension {
                    field: crate::RequestField::Image,
                })
            ));
        }
    }
}

#[test]
fn every_preparation_entry_point_rejects_an_oversized_target() {
    let source = image(2, 2);
    let field = crate::RequestField::Image;
    let adapter = CountingPreprocessor::default();
    let operation = ImagePreparation::Source(ImageSize {
        width: crate::ImageDecodePolicy::default().max_width + 1,
        height: 1,
    });
    for error in [
        DefaultImagePreprocessor
            .prepare(field, &source, operation)
            .unwrap_err(),
        super::prepare_image(None, field, &source, operation).unwrap_err(),
        super::prepare_image(Some(&adapter), field, &source, operation).unwrap_err(),
    ] {
        assert!(contains_limit(&error, crate::ImageLimit::Width));
    }
    assert!(adapter.operations.lock().unwrap().is_empty());
}

#[test]
fn custom_preprocessor_output_must_meet_default_budgets_after_invocation() {
    struct FixedPreprocessor(PreparedImage, Mutex<bool>);

    impl ImagePreprocessor for FixedPreprocessor {
        fn prepare(
            &self,
            _field: crate::RequestField,
            _image: &EncodedImage,
            _operation: ImagePreparation,
        ) -> Result<PreparedImage, BridgeError> {
            *self.1.lock().unwrap() = true;
            Ok(self.0.clone())
        }
    }

    let relaxed = crate::ImageDecodePolicy {
        max_text_chunk_bytes: crate::ImageDecodePolicy::default().max_text_chunk_bytes + 1,
        ..Default::default()
    };
    let png = create_png_with_comment_text(&"a".repeat(relaxed.max_text_chunk_bytes));
    let adapter = FixedPreprocessor(
        PreparedImage::from_png_with_policy(png, relaxed).unwrap(),
        Mutex::new(false),
    );
    let source = image(2, 2);
    let error = super::prepare_image(
        Some(&adapter),
        crate::RequestField::Image,
        &source,
        ImagePreparation::Source(source.size()),
    )
    .unwrap_err();
    assert!(*adapter.1.lock().unwrap());
    assert!(contains_limit(&error, crate::ImageLimit::TextChunkBytes));
}

#[test]
fn reference_canvas_preserves_the_supported_orientation_choices() {
    for (source, expected) in [
        (
            ImageSize {
                width: 2,
                height: 3,
            },
            ImageSize {
                width: 1024,
                height: 1536,
            },
        ),
        (
            ImageSize {
                width: 3,
                height: 2,
            },
            ImageSize {
                width: 1536,
                height: 1024,
            },
        ),
        (
            ImageSize {
                width: 1,
                height: 1,
            },
            ImageSize {
                width: 1472,
                height: 1472,
            },
        ),
    ] {
        assert_eq!(
            ImagePreparation::character_reference(source).target_size(),
            expected
        );
    }
}

fn contains_limit(
    error: &(dyn std::error::Error + 'static),
    limit: crate::media_policy::ImageLimit,
) -> bool {
    if error
        .downcast_ref::<crate::media_policy::ImagePolicyError>()
        .is_some_and(|error| error.limit == limit)
    {
        return true;
    }
    error
        .source()
        .is_some_and(|source| contains_limit(source, limit))
}

#[test]
fn png_structure_failures_are_rejected_by_every_validated_media_boundary() {
    let mut truncated = create_png_bytes(2, 2);
    truncated.truncate(truncated.len() - 12);
    assert!(EncodedImage::from_bytes(truncated.clone()).is_err());
    assert!(PreparedImage::from_png(truncated.clone()).is_err());
    assert!(decode_generated_images(truncated).is_err());
}

#[test]
fn media_limits_apply_before_pixels_are_allocated() {
    use crate::media_policy::{ImageDecodePolicy, ImageLimit};
    let policy = ImageDecodePolicy {
        max_width: 1,
        ..ImageDecodePolicy::default()
    };
    let bytes = create_png_bytes(2, 2);
    for error in [
        EncodedImage::from_bytes_with_policy(bytes.clone(), policy).unwrap_err(),
        PreparedImage::from_png_with_policy(bytes.clone(), policy).unwrap_err(),
        super::decode_generated_images_with_policy(bytes.clone(), policy).unwrap_err(),
    ] {
        assert!(contains_limit(&error, ImageLimit::Width));
    }

    let policy = ImageDecodePolicy {
        max_decoded_bytes: 15,
        ..ImageDecodePolicy::default()
    };
    assert!(contains_limit(
        &EncodedImage::from_bytes_with_policy(bytes, policy).unwrap_err(),
        ImageLimit::DecodedBytes
    ));

    // A large declared IHDR must be rejected before even reaching its stale CRC or image data.
    let mut header = create_png_bytes(1, 1);
    header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(contains_limit(
        &EncodedImage::from_bytes(header).unwrap_err(),
        ImageLimit::Width
    ));
}

#[test]
fn base64_budget_and_png_encoder_are_bounded_before_output_growth() {
    use crate::media_policy::{ImageDecodePolicy, ImageLimit};
    let policy = ImageDecodePolicy {
        max_encoded_bytes: 2,
        ..ImageDecodePolicy::default()
    };
    let error = EncodedImage::from_base64_with_policy(" A Q I D \n", policy).unwrap_err();
    assert!(contains_limit(&error, ImageLimit::EncodedBytes));
    let error =
        PreparedImage::from_pixels_with_policy(&image::DynamicImage::new_rgb8(2, 2), policy)
            .unwrap_err();
    assert!(matches!(error, BridgeError::Preparation(_)));
    assert!(contains_limit(&error, ImageLimit::EncodedBytes));
}

#[test]
fn png_input_keeps_sixteen_bit_channels_without_a_second_decoder() {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Sixteen);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC])
        .unwrap();
    writer.finish().unwrap();
    let image = EncodedImage::from_bytes(bytes).unwrap();
    assert_eq!(
        image.pixels().to_rgb16().as_raw(),
        &[0x1234, 0x5678, 0x9ABC]
    );
}

#[test]
fn palette_png_is_expanded_and_non_png_formats_share_the_format_mapping() {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(vec![10, 20, 30]);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0]).unwrap();
    writer.finish().unwrap();
    assert_eq!(
        EncodedImage::from_bytes(bytes)
            .unwrap()
            .pixels()
            .to_rgb8()
            .as_raw(),
        &[10, 20, 30]
    );

    for (format, expected) in [
        (image::ImageFormat::Jpeg, super::ImageMediaFormat::Jpeg),
        (image::ImageFormat::WebP, super::ImageMediaFormat::WebP),
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, format)
            .unwrap();
        let generated = decode_generated_images(bytes.into_inner())
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(generated.format(), expected);
        assert_eq!(generated.mime_type(), expected.mime_type());
        assert!(matches!(
            generated.metadata(),
            GeneratedImageMetadata::UnsupportedFormat
        ));
    }
}

#[test]
fn preparation_checks_default_budgets_before_invoking_a_custom_adapter() {
    use crate::media_policy::{ImageDecodePolicy, ImageLimit};
    let relaxed = ImageDecodePolicy {
        max_width: 20_000,
        ..ImageDecodePolicy::default()
    };
    let source =
        EncodedImage::from_bytes_with_policy(create_png_bytes(17_000, 1), relaxed).unwrap();
    let adapter = CountingPreprocessor::default();
    let field = crate::RequestField::Image;
    let operation = ImagePreparation::Source(ImageSize {
        width: 2,
        height: 2,
    });
    for error in [
        DefaultImagePreprocessor
            .prepare(field, &source, operation)
            .unwrap_err(),
        super::prepare_image(None, field, &source, operation).unwrap_err(),
        super::prepare_image(Some(&adapter), field, &source, operation).unwrap_err(),
    ] {
        assert!(contains_limit(&error, ImageLimit::Width));
    }
    assert!(adapter.operations.lock().unwrap().is_empty());
}

#[test]
fn rgb_webp_budget_accounts_for_the_backend_rgba_pixel_workspace() {
    let pixels =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30])));
    let mut output = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut output, image::ImageFormat::WebP)
        .unwrap();
    let policy = crate::ImageDecodePolicy {
        max_decoded_bytes: 12,
        ..Default::default()
    };
    let error = EncodedImage::from_bytes_with_policy(output.into_inner(), policy).unwrap_err();
    let mut source: &(dyn std::error::Error + 'static) = &error;
    loop {
        if let Some(error) = source.downcast_ref::<crate::ImagePolicyError>() {
            assert_eq!(error.limit, crate::ImageLimit::DecodedBytes);
            assert_eq!(error.actual, 16);
            break;
        }
        source = source
            .source()
            .expect("pixel workspace rejection retains its cause");
    }
}
