use super::*;
use crate::{VibeReference, VibeTransferConfig, generation::test_support::image};

#[test]
fn character_rows_share_preparation_and_keep_their_own_conditioning() {
    let image = image();
    let mut style =
        crate::CharacterReference::new(image.clone(), crate::CharacterReferenceType::Style);
    style.strength = 0.25;
    style.fidelity = 0.75;
    style.information_extracted = 0.5;
    let character = crate::CharacterReference::new(image, crate::CharacterReferenceType::Character);
    let references = ReferenceSet {
        character_references: vec![style, character],
        ..Default::default()
    };
    let prepared = prepare_references(Some(&references), None).unwrap();
    let ReferenceParameters::Character(rows) = &prepared else {
        panic!("character references")
    };
    assert_eq!(rows.len(), 2);
    assert!(std::ptr::eq(
        rows[0].image.bytes().as_ptr(),
        rows[1].image.bytes().as_ptr()
    ));
    let value = serde_json::to_value(&prepared).unwrap();
    assert_eq!(
        value["director_reference_descriptions"][0]["caption"]["base_caption"],
        "style"
    );
    assert_eq!(
        value["director_reference_descriptions"][1]["caption"]["base_caption"],
        "character"
    );
    assert_eq!(
        value["director_reference_strength_values"],
        serde_json::json!([0.25, 1.0])
    );
    assert_eq!(
        value["director_reference_secondary_strength_values"],
        serde_json::json!([0.25, 0.0])
    );
    assert_eq!(
        value["director_reference_information_extracted"],
        serde_json::json!([0.5, 1.0])
    );
}

#[test]
fn repeated_default_references_share_prepared_bytes_without_merging_rows() {
    let image = image();
    let references = ReferenceSet {
        vibe_transfer: Some(VibeTransferConfig::new(vec![
            VibeReference::image(image.clone(), 0.25, 0.5),
            VibeReference::image(image, 0.75, -0.5),
        ])),
        ..Default::default()
    };
    let prepared = prepare_references(Some(&references), None).unwrap();
    let ReferenceParameters::Vibe { references, .. } = &prepared else {
        panic!("raw vibes")
    };
    assert_eq!(references.len(), 2);
    assert!(std::ptr::eq(
        references[0].image.bytes().as_ptr(),
        references[1].image.bytes().as_ptr()
    ));
    let value = serde_json::to_value(&prepared).unwrap();
    assert_eq!(
        value["reference_strength_multiple"],
        serde_json::json!([0.5, -0.5])
    );
    assert_eq!(
        value["reference_information_extracted_multiple"],
        serde_json::json!([0.25, 0.75])
    );
}

#[test]
fn custom_preprocessors_keep_each_rows_field_and_output() {
    struct ByIndex(std::sync::Mutex<Vec<crate::RequestField>>);
    impl ImagePreprocessor for ByIndex {
        fn prepare(
            &self,
            field: crate::RequestField,
            _: &crate::EncodedImage,
            operation: ImagePreparation,
        ) -> Result<crate::PreparedImage, BridgeError> {
            let mut calls = self.0.lock().unwrap();
            calls.push(field);
            let size = operation.target_size();
            let red = u8::try_from(calls.len()).unwrap();
            drop(calls);
            crate::PreparedImage::from_pixels(&image::DynamicImage::ImageRgba8(
                image::RgbaImage::from_pixel(
                    size.width,
                    size.height,
                    image::Rgba([red, 0, 0, 255]),
                ),
            ))
        }
    }
    let image = image();
    let references = ReferenceSet {
        vibe_transfer: Some(VibeTransferConfig::new(vec![
            VibeReference::image(image.clone(), 0.5, 1.0),
            VibeReference::image(image, 0.5, 1.0),
        ])),
        ..Default::default()
    };
    let preprocessor = ByIndex(std::sync::Mutex::new(Vec::new()));
    let prepared = prepare_references(Some(&references), Some(&preprocessor)).unwrap();
    let calls = preprocessor.0.lock().unwrap();
    assert_eq!(
        calls.as_slice(),
        [
            GenerationField::VibeReference {
                index: 0,
                field: VibeReferenceField::Image
            }
            .into(),
            GenerationField::VibeReference {
                index: 1,
                field: VibeReferenceField::Image
            }
            .into(),
        ]
    );
    let ReferenceParameters::Vibe { references, .. } = prepared else {
        panic!("raw vibes")
    };
    assert_ne!(references[0].image.bytes(), references[1].image.bytes());
}

#[test]
fn distinct_operations_on_shared_pixels_keep_distinct_preparations() {
    let image = image();
    let mut images = ReferenceImages {
        preprocessor: None,
        prepared: HashMap::new(),
    };
    let source = images
        .prepare(
            crate::RequestField::Image,
            &image,
            ImagePreparation::Source(crate::ImageSize {
                width: 8,
                height: 8,
            }),
        )
        .unwrap();
    let mask = images
        .prepare(
            crate::RequestField::Image,
            &image,
            ImagePreparation::Mask(crate::ImageSize {
                width: 8,
                height: 8,
            }),
        )
        .unwrap();
    assert!(!std::ptr::eq(
        source.as_bytes().as_ptr(),
        mask.as_bytes().as_ptr()
    ));
    let larger = images
        .prepare(
            crate::RequestField::Image,
            &image,
            ImagePreparation::Source(crate::ImageSize {
                width: 9,
                height: 8,
            }),
        )
        .unwrap();
    assert_eq!(
        larger.size(),
        crate::ImageSize {
            width: 9,
            height: 8
        }
    );
    assert_ne!(larger.as_bytes(), source.as_bytes());
}
