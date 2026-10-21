use super::*;

#[test]
fn accepts_data_url() {
    let payload = decode_image("test.image", "data:image/png;base64,QUJDRA==").unwrap();
    assert_eq!(payload, b"ABCD");
}

#[test]
fn rejects_non_image_data_url() {
    let error =
        decode_image("test.image", "data:text/plain;base64,QUJDRA==").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidDataUrl {
            field,
            reason: DataUrlError::UnsupportedMediaType,
        }) if field == "test.image"
    ));
}

#[test]
fn rejects_empty_input() {
    let error = decode_image("test.image", "   ").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { field })
            if field == "test.image"
    ));
}
