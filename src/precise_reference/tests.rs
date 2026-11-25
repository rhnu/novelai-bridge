use serde_json::json;

use super::*;

fn png(red: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Rgb);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[red, 0, 0]).unwrap();
    writer.finish().unwrap();
    bytes
}

fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in members {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn reads_manifest_order_and_original_images_with_legacy_enabled_defaults() {
    let first = png(30);
    let second = png(198);
    let metadata = serde_json::to_vec(&json!([
        {"filename":"服装.v2.png", "description":"costume", "information_extracted":0.35, "fidelity":1.4, "strength":-0.8, "enabled":false},
        {"filename":"original.png", "description":"character&style_anyoutfit", "information_extracted":1.0, "fidelity":0.5, "strength":0.9},
        {"filename":"old.png", "description":"character", "information_extracted":1.0, "fidelity":1.0, "strength":1.0, "enabled":null}
    ])).unwrap();
    let zip = archive(&[
        ("original.png", &second),
        ("old.png", &second),
        ("metadata.json", &metadata),
        ("服装.v2.png", &first),
    ]);
    let document = PreciseReferenceDocument::from_bytes(&zip).unwrap();
    assert_eq!(document.entries.len(), 3);
    let entry = &document.entries[0];
    assert_eq!(entry.filename, "服装.v2.png");
    assert!(!entry.enabled);
    assert_eq!(
        entry.reference.reference_type,
        CharacterReferenceType::Costume
    );
    assert_eq!(entry.reference.image.as_bytes(), first);
    assert!((entry.reference.information_extracted - 0.35).abs() < f32::EPSILON);
    assert!((entry.reference.fidelity - 1.4).abs() < f32::EPSILON);
    assert!((entry.reference.strength + 0.8).abs() < f32::EPSILON);
    assert_eq!(document.entries[1].filename, "original.png");
    assert_eq!(
        document.entries[1].reference.reference_type,
        CharacterReferenceType::CharacterAndStyleAnyOutfit
    );
    assert_eq!(document.entries[1].reference.image.as_bytes(), second);
    assert!(document.entries[1..].iter().all(|entry| entry.enabled));
}

#[test]
fn writes_unique_filenames_and_explicit_enabled_state_in_the_official_manifest() {
    let first = png(30);
    let second = png(198);
    let entry = |name: &str, bytes: &[u8], enabled| PreciseReferenceEntry {
        filename: name.to_owned(),
        enabled,
        reference: CharacterReference::new(
            EncodedImage::from_bytes(bytes).unwrap(),
            CharacterReferenceType::CharacterMainOutfit,
        ),
    };
    let document = PreciseReferenceDocument {
        entries: vec![
            entry("reference.v2.png", &first, true),
            entry("reference.v2.png", &second, false),
        ],
    };

    let output = document.to_bytes().unwrap();
    let mut zip = ZipArchive::new(Cursor::new(&output)).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_reader(zip.by_name("metadata.json").unwrap()).unwrap();
    assert_eq!(
        manifest,
        json!([
            {"filename":"reference.v2.png", "description":"character_mainoutfit", "information_extracted":1.0, "fidelity":1.0, "strength":1.0, "enabled":true},
            {"filename":"reference.v2_1.png", "description":"character_mainoutfit", "information_extracted":1.0, "fidelity":1.0, "strength":1.0, "enabled":false},
        ])
    );
    let mut bytes = Vec::new();
    zip.by_name("reference.v2_1.png")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, second);
    let mut expected = document;
    expected.entries[1].filename = "reference.v2_1.png".to_owned();
    assert_eq!(
        PreciseReferenceDocument::from_bytes(&output).unwrap(),
        expected
    );
}

#[test]
fn reports_the_missing_reference_or_unsafe_filename() {
    let metadata = |filename| {
        serde_json::to_vec(&json!([
        {"filename":filename, "description":"style", "information_extracted":1.0, "fidelity":1.0, "strength":1.0}
    ])).unwrap()
    };
    let zip = archive(&[("metadata.json", &metadata("absent.png"))]);
    assert!(matches!(
        PreciseReferenceDocument::from_bytes(&zip),
        Err(PreciseReferenceDocumentError::MissingImage { index: 0 })
    ));
    let zip = archive(&[("metadata.json", &metadata("../reference.png"))]);
    assert!(matches!(
        PreciseReferenceDocument::from_bytes(&zip),
        Err(PreciseReferenceDocumentError::InvalidFilename { index: 0 })
    ));
}

#[test]
fn enforces_manifest_archive_and_cumulative_media_budgets() {
    let bytes = png(13);
    let metadata = serde_json::to_vec(&json!([
        {"filename":"first.png", "description":"style", "information_extracted":1.0, "fidelity":1.0, "strength":1.0},
        {"filename":"second.png", "description":"style", "information_extracted":1.0, "fidelity":1.0, "strength":1.0}
    ])).unwrap();
    let zip = archive(&[
        ("metadata.json", &metadata),
        ("first.png", &bytes),
        ("second.png", &bytes),
    ]);
    let read = |policy| PreciseReferenceDocument::from_bytes_with_policy(&zip, policy);
    assert!(matches!(
        read(ImageDecodePolicy {
            max_text_chunk_bytes: metadata.len() - 1,
            ..ImageDecodePolicy::default()
        }),
        Err(PreciseReferenceDocumentError::ManifestTooLarge { .. })
    ));
    assert!(matches!(
        read(ImageDecodePolicy {
            max_archive_entries: 2,
            ..ImageDecodePolicy::default()
        }),
        Err(PreciseReferenceDocumentError::Limit(ImagePolicyError {
            limit: ImageLimit::ArchiveEntries,
            ..
        }))
    ));
    assert!(matches!(
        read(ImageDecodePolicy {
            max_decoded_bytes: 4,
            ..ImageDecodePolicy::default()
        }),
        Err(PreciseReferenceDocumentError::Image { index: 1, .. })
    ));
}
