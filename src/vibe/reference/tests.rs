use super::*;
use crate::{CharacterReference, CharacterReferenceType, generation::test_support::image};

#[test]
fn reference_inputs_roundtrip_and_reject_unknown_kinds_and_options() {
    for reference in [
        VibeReference::encoded(VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap(), -2.0),
        VibeReference::image(image(), 0.5, 3.0),
    ] {
        let mut value = serde_json::to_value(&reference).unwrap();
        assert_eq!(
            serde_json::from_value::<VibeReference>(value.clone()).unwrap(),
            reference
        );
        value["input"]["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<VibeReference>(value).is_err());
    }
    let invalid = serde_json::json!({ "input": { "kind": "unknown" }, "strength": 1.0 });
    assert!(serde_json::from_value::<VibeReference>(invalid).is_err());
}

#[test]
fn defaults_match_the_provider_reference_settings() {
    let config: VibeTransferConfig =
        serde_json::from_value(serde_json::json!({ "references": [] })).unwrap();
    assert!(config.normalize_strength);
    assert_eq!(config, VibeTransferConfig::new(vec![]));
    let mut value = serde_json::to_value(CharacterReference::new(
        image(),
        CharacterReferenceType::Character,
    ))
    .unwrap();
    value
        .as_object_mut()
        .unwrap()
        .remove("information_extracted");
    let reference: CharacterReference = serde_json::from_value(value).unwrap();
    assert_eq!(reference.information_extracted, 1.0);
}
