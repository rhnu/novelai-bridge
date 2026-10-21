use serde_json::json;

use super::*;

#[test]
fn edits_share_array_and_object_paths_without_creating_parents() {
    let mut value = json!({"parameters":{"refs":[{"strength":0.3},{"strength":0.7}]}});
    assert!(set_path(
        &mut value,
        "parameters.refs.1.strength",
        json!(0.5)
    ));
    assert!(set_path(&mut value, "parameters.extra", json!(true)));
    assert!(!set_path(&mut value, "missing.extra", json!(1)));
    assert!(!set_path(&mut value, "parameters.refs.9", json!(1)));
    assert!(!remove_path(&mut value, "parameters.refs.9"));
    assert!(remove_path(&mut value, "parameters.refs.0"));
    assert_eq!(
        value,
        json!({"parameters":{"refs":[{"strength":0.5}],"extra":true}})
    );
}

#[test]
fn empty_segments_and_missing_targets_fail_without_mutation() {
    let mut value = json!({"parameters":{}});
    for path in ["", ".parameters", "parameters.", "parameters..field"] {
        assert!(!set_path(&mut value, path, json!(true)));
        assert!(!remove_path(&mut value, path));
    }
    assert_eq!(value, json!({"parameters":{}}));
}
