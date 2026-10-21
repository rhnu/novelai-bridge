use base64::Engine;

use crate::{VibeDocument, generation::test_support::image};

fn document() -> VibeDocument {
    VibeDocument::from_json(
        br#"{
        "identifier":"novelai-vibe-transfer","version":1,"type":"encoding",
        "id":"b70035bb783a47bf61ac3ff70b005308e167ee984365690e638c1481b8ca2936",
        "encodings":{"v4full":{"z-first":{"encoding":"AQID"},"a-second":{"encoding":"BAUG"}}}
    }"#,
    )
    .unwrap()
}

#[test]
fn document_png_roundtrip_retains_cache_order_identity_and_existing_text() {
    let source = image();
    assert_eq!(VibeDocument::from_png(source.as_bytes()).unwrap(), None);
    let doc = document();
    let with_title = crate::write_png_text_chunks(
        source.as_bytes(),
        &[crate::PngTextEntry::new("Title", "keep this title")],
    )
    .unwrap();
    let embedded = doc.embed_in_png(&with_title).unwrap();
    let recovered = VibeDocument::from_png(&embedded).unwrap().unwrap();
    assert_eq!(doc, recovered);
    let raw = crate::extract_embedded_vibe_document_from_png_bytes(&embedded)
        .unwrap()
        .unwrap();
    assert!(raw.find("z-first").unwrap() < raw.find("a-second").unwrap());
    let metadata = crate::parse_png_metadata_from_bytes(&embedded).unwrap();
    let encoded = &metadata.text_chunks["naidata"][0];
    assert!(!encoded.starts_with('{'));
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap(),
        doc.to_json().unwrap()
    );
    assert_eq!(metadata.text_chunks["Title"], ["keep this title"]);
    assert_eq!(metadata.vibe_entries[0].model_encodings["v4full"].len(), 2);
    assert_eq!(doc.embed_in_png(&embedded).unwrap(), embedded);
}

#[test]
fn invalid_document_and_invalid_png_cannot_be_exported_as_success() {
    let mut doc = document();
    let VibeDocument::Single(file) = &mut doc else {
        panic!("single fixture")
    };
    file.id = "0".repeat(64);
    let error = doc.embed_in_png(image().as_bytes()).unwrap_err();
    assert!(matches!(
        error,
        crate::BridgeError::Metadata(crate::MetadataError::InvalidVibeDocument { .. })
    ));
    assert!(document().embed_in_png(b"not a PNG").is_err());
    let oversized = VibeDocument::Single(
        crate::VibeFile::encoded("v4full", "A".repeat(2 * 1024 * 1024)).into(),
    );
    assert!(oversized.embed_in_png(image().as_bytes()).is_err());
}
