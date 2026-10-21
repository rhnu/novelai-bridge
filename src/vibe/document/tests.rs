use serde_json::json;

use super::*;

const ENCODING: &[u8] = br#"{
  "identifier":"novelai-vibe-transfer","version":1,"type":"encoding",
  "id":"b70035bb783a47bf61ac3ff70b005308e167ee984365690e638c1481b8ca2936",
  "encodings":{
    "z-model":{"z-params":{"encoding":"AQID","params":{"information_extracted":0.7,"mask":"mask-original"}},"a-params":{"encoding":"BAUG"}},
    "a-model":{"unknown":{"encoding":"BwgJ"}}
  },
  "name":"reference","createdAt":1720000000123,
  "importInfo":{"model":"future-provider-model","information_extracted":0.7,"strength":-2.5,"mask":"original-import-mask"},
  "future-extension":{"ignored":[1,2,3]}
}"#;

fn file() -> VibeFile {
    let VibeDocument::Single(file) = VibeDocument::from_json(ENCODING).unwrap() else {
        panic!("single fixture")
    };
    *file
}

#[test]
fn official_nested_cache_retains_model_order_parameter_order_and_complete_records() {
    let document = VibeDocument::from_json(ENCODING).unwrap();
    let file = &document.files()[0];
    assert_eq!(
        file.encodings
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z-model", "a-model"]
    );
    assert_eq!(
        file.encodings["z-model"]
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["z-params", "a-params"]
    );
    assert_eq!(
        file.encodings["z-model"]["z-params"]
            .params
            .as_ref()
            .unwrap()
            .mask
            .as_deref(),
        Some("mask-original")
    );
    assert_eq!(
        file.import_info.as_ref().unwrap().model.as_deref(),
        Some("future-provider-model")
    );
    assert_eq!(file.import_info.as_ref().unwrap().strength, Some(-2.5));
    assert_eq!(file.created_at, Some(1_720_000_000_123));
    let exported = document.to_json().unwrap();
    let text = std::str::from_utf8(&exported).unwrap();
    assert!(text.find("z-model").unwrap() < text.find("a-model").unwrap());
    assert!(text.find("z-params").unwrap() < text.find("a-params").unwrap());
    assert_eq!(VibeDocument::from_json(&exported).unwrap(), document);
}

#[test]
fn identity_hashes_the_original_string_and_selection_alone_decodes_base64() {
    let encoding = VibeFile::encoded("v4full", "Q Q = = ");
    assert_eq!(
        encoding.id,
        "3bba350710979bf82fb45ecd9fc6bb7e92f7098545a72e55a192e101bc97e5d1"
    );
    let document = VibeDocument::Single(Box::new(encoding));
    let recovered = VibeDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(
        recovered.files()[0].encodings["v4full"]["unknown"].encoding,
        "Q Q = = "
    );
    assert_eq!(
        recovered.files()[0]
            .encoding("v4full", "unknown")
            .unwrap()
            .unwrap()
            .as_bytes(),
        b"A"
    );
    let image = VibeFile::image("data:image/png;base64,AQID");
    assert_eq!(
        image.id,
        "8a37c75fc31bfff9652678f50164cf27b31c787b8a946f87f1cd3cbf61365db1"
    );
    let recovered =
        VibeDocument::from_json(&VibeDocument::Single(Box::new(image)).to_json().unwrap()).unwrap();
    assert_eq!(
        recovered.files()[0].image.as_deref(),
        Some("data:image/png;base64,AQID")
    );
}

#[test]
fn image_files_keep_their_full_cache_and_missing_optional_import_settings() {
    let mut image = VibeFile::image("original-image-string");
    image.encodings = file().encodings;
    let document = VibeDocument::Single(Box::new(image));
    let roundtrip = VibeDocument::from_json(&document.to_json().unwrap()).unwrap();
    let image = &roundtrip.files()[0];
    assert_eq!(image.encodings.len(), 2);
    assert_eq!(image.encodings["z-model"].len(), 2);
    assert_eq!(image.import_info, None);
    assert_eq!(image.created_at, None);
    assert!(image.encoding("z-model", "missing").unwrap().is_none());
    assert!(image.encoding("missing", "z-params").unwrap().is_none());
    assert_eq!(
        image
            .encoding("z-model", "a-params")
            .unwrap()
            .unwrap()
            .as_bytes(),
        &[4, 5, 6]
    );
}

#[test]
fn bundle_roundtrips_without_merging_and_invalid_entries_fail_atomically() {
    let first = VibeFile::encoded("v4full", "AQID");
    let second = VibeFile::image("different original image");
    let document = VibeDocument::Bundle(VibeBundle::new(vec![first, second]));
    let roundtrip = VibeDocument::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, document);
    assert_eq!(roundtrip.files().len(), 2);
    let mut value = serde_json::to_value(&document).unwrap();
    value["vibes"][1]["id"] = json!("0".repeat(64));
    assert!(matches!(
        VibeDocument::from_json(&serde_json::to_vec(&value).unwrap()),
        Err(VibeDocumentError::HashMismatch { entry: 1 })
    ));
}

#[test]
fn malformed_identity_versions_and_missing_type_payloads_are_errors() {
    let original = serde_json::to_value(VibeFile::encoded("v4full", "AQID")).unwrap();
    for (key, value) in [
        ("identifier", json!("not-a-vibe")),
        ("version", json!(2)),
        ("type", json!("custom")),
        ("id", json!("UPPERCASE")),
        ("encodings", json!({})),
    ] {
        let mut modified = original.clone();
        modified[key] = value;
        assert!(
            VibeDocument::from_json(&serde_json::to_vec(&modified).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut image = VibeFile::image("original");
    image.image = None;
    assert!(matches!(
        VibeDocument::Single(Box::new(image)).validate(),
        Err(VibeDocumentError::MissingImage { entry: 0 })
    ));
    let mut file = file();
    file.encodings.swap_indices(0, 1);
    assert!(matches!(
        VibeDocument::Single(Box::new(file)).validate(),
        Err(VibeDocumentError::HashMismatch { entry: 0 })
    ));
}

#[test]
fn all_resource_budgets_apply_to_import_and_export() {
    let document = VibeDocument::Single(Box::new(VibeFile::encoded("v4full", "AQID")));
    let json = document.to_json().unwrap();
    for (limits, resource) in [
        (
            VibeDocumentLimits {
                max_document_bytes: json.len() - 1,
                ..Default::default()
            },
            VibeDocumentResource::DocumentBytes,
        ),
        (
            VibeDocumentLimits {
                max_vibes: 0,
                ..Default::default()
            },
            VibeDocumentResource::Vibes,
        ),
        (
            VibeDocumentLimits {
                max_cache_entries: 1,
                ..Default::default()
            },
            VibeDocumentResource::CacheEntries,
        ),
        (
            VibeDocumentLimits {
                max_payload_bytes: 3,
                ..Default::default()
            },
            VibeDocumentResource::PayloadBytes,
        ),
        (
            VibeDocumentLimits {
                max_total_payload_bytes: 3,
                ..Default::default()
            },
            VibeDocumentResource::TotalPayloadBytes,
        ),
    ] {
        for result in [
            VibeDocument::from_json_with_limits(&json, limits).map(|_| ()),
            document.to_json_with_limits(limits).map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(VibeDocumentError::Limit { resource: actual, .. }) if actual == resource)
            );
        }
    }
    assert!(matches!(
        VibeDocument::from_json_with_limits(
            b"not JSON",
            VibeDocumentLimits {
                max_document_bytes: 1,
                ..Default::default()
            }
        ),
        Err(VibeDocumentError::Limit {
            resource: VibeDocumentResource::DocumentBytes,
            ..
        })
    ));
}

#[test]
fn preflight_rejects_collection_overflow_before_parsing_an_invalid_next_record() {
    let bundle = json!({"identifier":"novelai-vibe-transfer-bundle","version":1,"vibes":[serde_json::to_value(VibeFile::encoded("v4full","AQID")).unwrap(), {"encodings":"invalid"}]});
    let limits = VibeDocumentLimits {
        max_vibes: 1,
        ..Default::default()
    };
    assert!(matches!(
        VibeDocument::from_json_with_limits(&serde_json::to_vec(&bundle).unwrap(), limits),
        Err(VibeDocumentError::Limit {
            resource: VibeDocumentResource::Vibes,
            ..
        })
    ));
    let file =
        json!({"identifier":"novelai-vibe-transfer", "encodings":{"model":{"record":"invalid"}}});
    let limits = VibeDocumentLimits {
        max_cache_entries: 1,
        ..Default::default()
    };
    assert!(matches!(
        VibeDocument::from_json_with_limits(&serde_json::to_vec(&file).unwrap(), limits),
        Err(VibeDocumentError::Limit {
            resource: VibeDocumentResource::CacheEntries,
            ..
        })
    ));
}

#[test]
fn selected_invalid_base64_is_an_error_and_is_never_treated_as_an_image() {
    let file = VibeFile::encoded("unknown-model", "!invalid!");
    assert!(matches!(
        file.encoding("unknown-model", "unknown"),
        Err(VibeDocumentError::InvalidEncoding { .. })
    ));
}

#[test]
fn editable_nonfinite_values_and_invalid_thumbnails_do_not_export() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut file = file();
        file.import_info.as_mut().unwrap().strength = Some(number);
        assert!(matches!(
            VibeDocument::Single(Box::new(file)).to_json(),
            Err(VibeDocumentError::NonFiniteNumber { entry: 0, .. })
        ));
    }
    let mut file = file();
    file.thumbnail = Some("https://example.invalid/private".to_owned());
    assert!(matches!(
        VibeDocument::Single(Box::new(file)).validate(),
        Err(VibeDocumentError::InvalidThumbnail { entry: 0 })
    ));
}

#[test]
fn debug_and_error_display_do_not_expose_original_payloads() {
    let mut file = VibeFile::image("private-image-marker");
    file.thumbnail = Some("private-thumbnail-marker".to_owned());
    file.import_info = Some(VibeImportInfo {
        mask: Some("private-mask-marker".to_owned()),
        ..Default::default()
    });
    let record = VibeEncodingRecord {
        encoding: "private-encoding-marker".to_owned(),
        params: Some(VibeEncodingParams {
            mask: Some("private-params-mask-marker".to_owned()),
            ..Default::default()
        }),
    };
    for debug in [
        format!("{file:?}"),
        format!("{record:?}"),
        format!("{:?}", record.params),
        format!("{:?}", file.import_info),
        format!("{:?}", VibeDocument::Single(Box::new(file.clone()))),
        format!("{:?}", VibeBundle::new(vec![file.clone()])),
    ] {
        assert!(!debug.contains("private-"));
    }
    assert!(
        !VibeDocument::Single(Box::new(file))
            .validate()
            .unwrap_err()
            .to_string()
            .contains("private-")
    );
}
