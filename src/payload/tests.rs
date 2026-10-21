use super::*;

#[test]
fn accepts_data_url() {
    let payload = decode_image(RequestField::Image, "data:image/png;base64,QUJDRA==").unwrap();
    assert_eq!(payload, b"ABCD");
}

#[test]
fn rejects_non_image_data_url() {
    let error = decode_image(RequestField::Image, "data:text/plain;base64,QUJDRA==").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::InvalidDataUrl {
            field,
            reason: DataUrlError::UnsupportedMediaType,
        }) if field == RequestField::Image
    ));
}

#[test]
fn rejects_empty_input() {
    let error = decode_image(RequestField::Image, "   ").unwrap_err();
    assert!(matches!(
        error,
        BridgeError::InvalidRequest(InvalidRequest::EmptyField { field })
            if field == RequestField::Image
    ));
}

#[test]
fn data_url_requires_an_exact_final_base64_marker() {
    for metadata in [
        "image/png;base64junk",
        "image/png;base64=anything",
        "image/png;base64;charset=utf-8",
        "image/;base64",
    ] {
        assert!(decode_image(RequestField::Image, &format!("data:{metadata},QUJDRA==")).is_err());
    }
    assert_eq!(
        decode_image(
            RequestField::Image,
            "data:image/png;charset=utf-8;base64,QUJDRA=="
        )
        .unwrap(),
        b"ABCD"
    );
}
