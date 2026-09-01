mod support;

use std::{io::Write, sync::Mutex};

use novelai_bridge::{
    BridgeError, CharacterReference, CharacterReferenceType, DefaultImagePreprocessor,
    EncodedImage, GenerateImageRequest, GeneratedImageMetadata, ImagePreparation,
    ImagePreprocessor, ImageSize, Img2ImgRequest, Model, PreparedImage, VibeEncoding,
    decode_generated_images,
};

fn image(width: u32, height: u32) -> EncodedImage {
    EncodedImage::from_bytes(support::create_png_bytes(width, height)).unwrap()
}

#[test]
fn encoded_image_validates_once_and_serializes_as_canonical_base64() {
    let bytes = support::create_png_bytes(3, 2);
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
fn vibe_encoding_is_opaque_nonempty_data_not_an_image() {
    let encoding = VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap();
    assert_eq!(encoding.to_base64(), "AQID");
    assert_eq!(VibeEncoding::from_base64(" A Q I D \n").unwrap(), encoding);
    assert!(VibeEncoding::from_bytes(Vec::new()).is_err());
}

#[test]
fn prepared_image_constructor_keeps_custom_implementations_validated() {
    let prepared = PreparedImage::from_png(support::create_png_bytes(5, 4)).unwrap();
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
    let png = support::create_png_bytes(2, 2);
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
    writer.write_all(&support::create_png_bytes(2, 2)).unwrap();
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
    let png = support::create_png_with_comment_text("not json");
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

#[test]
fn custom_preprocessor_observes_each_semantic_input_once() {
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
            ImagePreparation::CharacterReference,
        ]
    );
}
