use super::*;

#[test]
fn normalize_image_payload_accepts_data_url() {
    let payload = normalize_image_payload("test.image", "data:image/png;base64,QUJDRA==").unwrap();
    assert_eq!(payload, "QUJDRA==");
}

#[test]
fn normalize_image_payload_rejects_non_image_data_url() {
    let error =
        normalize_image_payload("test.image", "data:text/plain;base64,QUJDRA==").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidDataUrl {
            field,
            reason: DataUrlError::UnsupportedMediaType,
        }) if field == "test.image"
    ));
}

#[test]
fn normalize_image_payload_rejects_empty_input() {
    let error = normalize_image_payload("test.image", "   ").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { field })
            if field == "test.image"
    ));
}
