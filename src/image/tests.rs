use std::{io::Write, sync::Mutex};

use crate::{
    BridgeError, DefaultImagePreprocessor,
    EncodedImage, GeneratedImageMetadata, ImagePreparation,
    ImagePreprocessor, ImageSize, PreparedImage,
    decode_generated_images,
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
    assert_eq!(images[0].bytes, png);
    assert_eq!(images[0].mime_type.as_deref(), Some("image/png"));
    assert!(decode_generated_images(b"not an image".to_vec()).is_err());
}

#[test]
fn one_invalid_image_candidate_rejects_the_whole_archive() {
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
    assert!(matches!(image.metadata, GeneratedImageMetadata::Parsed(_)));
}

#[derive(Default)]
struct CountingPreprocessor {
    operations: Mutex<Vec<ImagePreparation>>,
}

impl ImagePreprocessor for CountingPreprocessor {
    fn prepare(
        &self,
        field: &str,
        image: &EncodedImage,
        operation: ImagePreparation,
    ) -> Result<PreparedImage, BridgeError> {
        self.operations.lock().unwrap().push(operation);
        DefaultImagePreprocessor.prepare(field, image, operation)
    }
}

#[cfg(feature = "generation")]
#[test]
fn custom_preprocessor_observes_each_semantic_input_once() {
    use crate::{CharacterReference, CharacterReferenceType, GenerateImageRequest, Img2ImgRequest, Model};
    let mut img2img = Img2ImgRequest::new(image(4, 3), 0.5, 0.0);
    img2img.mask = Some(image(4, 3));
    let request = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
        .size(ImageSize {
            width: 64,
            height: 64,
        })
        .img2img(img2img)
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
            ImagePreparation::character_reference(ImageSize { width: 3, height: 4 }),
        ]
    );
}


fn create_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&vec![255; (width * height * 4) as usize]).unwrap();
    writer.finish().unwrap();
    bytes
}

fn create_png_with_comment_text(comment: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.add_text_chunk("Comment".to_owned(), comment.to_owned()).unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[255; 16]).unwrap();
    writer.finish().unwrap();
    bytes
}

struct PixelCopyPreprocessor;
impl ImagePreprocessor for PixelCopyPreprocessor {
    fn prepare(&self, _field: &str, image: &EncodedImage, _operation: ImagePreparation) -> Result<PreparedImage, BridgeError> {
        let mut output = std::io::Cursor::new(Vec::new());
        image.pixels().write_to(&mut output, image::ImageFormat::Png).unwrap();
        PreparedImage::from_png(output.into_inner())
    }
}

#[test]
fn custom_preprocessor_reuses_pixels_and_must_match_the_resolved_canvas() {
    let image = image(3, 2);
    let prepared = super::prepare_image(&PixelCopyPreprocessor, "source", &image, ImagePreparation::Source(image.size())).unwrap();
    assert_eq!(prepared.size(), image.size());
    let expected = ImageSize { width: 8, height: 8 };
    assert!(matches!(super::prepare_image(&PixelCopyPreprocessor, "source", &image, ImagePreparation::Mask(expected)), Err(BridgeError::InvalidRequest(crate::InvalidRequest::PreparedImageSizeMismatch { expected: actual_target, actual, .. })) if actual_target == expected && actual == image.size()));
}

#[test]
fn preprocessing_rejects_zero_canvas_before_calling_the_adapter() {
    let preprocessor = CountingPreprocessor::default();
    assert!(super::prepare_image(&preprocessor, "source", &image(2, 2), ImagePreparation::Source(ImageSize { width: 0, height: 2 })).is_err());
    assert!(preprocessor.operations.lock().unwrap().is_empty());
}

#[test]
fn reference_canvas_preserves_the_supported_orientation_choices() {
    for (source, expected) in [
        (ImageSize { width: 2, height: 3 }, ImageSize { width: 1024, height: 1536 }),
        (ImageSize { width: 3, height: 2 }, ImageSize { width: 1536, height: 1024 }),
        (ImageSize { width: 1, height: 1 }, ImageSize { width: 1472, height: 1472 }),
    ] {
        assert_eq!(ImagePreparation::character_reference(source).target_size(), expected);
    }
}
