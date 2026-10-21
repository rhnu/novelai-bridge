use super::*;

#[test]
fn accepts_data_url() {
    let payload = decode_image(
        RequestField::Image,
        "data:image/png;base64,QUJDRA==",
        ImageDecodePolicy::default(),
    )
    .unwrap();
    assert_eq!(payload, b"ABCD");
}

#[test]
fn rejects_non_image_data_url() {
    let error = decode_image(
        RequestField::Image,
        "data:text/plain;base64,QUJDRA==",
        ImageDecodePolicy::default(),
    )
    .unwrap_err();
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
    let error = decode_image(RequestField::Image, "   ", ImageDecodePolicy::default()).unwrap_err();
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
        assert!(
            decode_image(
                RequestField::Image,
                &format!("data:{metadata},QUJDRA=="),
                ImageDecodePolicy::default()
            )
            .is_err()
        );
    }
    assert_eq!(
        decode_image(
            RequestField::Image,
            "data:image/png;charset=utf-8;base64,QUJDRA==",
            ImageDecodePolicy::default(),
        )
        .unwrap(),
        b"ABCD"
    );
}

#[test]
fn base64_whitespace_and_padding_preserve_exact_budget_boundaries() {
    for (payload, expected) in [
        ("QQ==", b"A".as_slice()),
        (" Q\u{2003}Q = =\n", b"A".as_slice()),
        ("QUI=", b"AB".as_slice()),
        ("Q U\tI\u{a0}=", b"AB".as_slice()),
        ("QUJD", b"ABC".as_slice()),
        ("QU J\r\nD", b"ABC".as_slice()),
    ] {
        let policy = ImageDecodePolicy {
            max_encoded_bytes: expected.len() as u64,
            ..ImageDecodePolicy::default()
        };
        assert_eq!(
            decode_image(RequestField::Image, payload, policy).unwrap(),
            expected
        );
        let smaller = ImageDecodePolicy {
            max_encoded_bytes: policy.max_encoded_bytes - 1,
            ..policy
        };
        let error = decode_image(RequestField::Image, payload, smaller).unwrap_err();
        let BridgeError::InvalidRequest(InvalidRequest::UndecodableImage { source, .. }) = error
        else {
            panic!("expected image budget rejection")
        };
        assert_eq!(
            source.downcast_ref::<ImagePolicyError>().unwrap().limit,
            crate::ImageLimit::EncodedBytes
        );
    }
}

#[test]
fn invalid_base64_is_bounded_before_decoding_with_field_specific_errors() {
    let policy = ImageDecodePolicy {
        max_encoded_bytes: 3,
        ..ImageDecodePolicy::default()
    };
    for input in [
        "🙂🙂🙂🙂",
        " \u{2003}🙂 🙂 🙂 🙂 ",
        "QUJDRA==",
        "Q U J D R A = =",
    ] {
        let error = decode_image(RequestField::Image, input, policy).unwrap_err();
        assert!(matches!(
            error,
            BridgeError::InvalidRequest(InvalidRequest::UndecodableImage { .. })
        ));
        let error = decode_image(RequestField::MetadataImage, input, policy).unwrap_err();
        assert!(matches!(
            error,
            BridgeError::Metadata(crate::MetadataError::InvalidPngPayload { .. })
        ));
    }
    assert!(matches!(
        decode_image(RequestField::Image, "????", policy),
        Err(BridgeError::InvalidRequest(
            InvalidRequest::InvalidBase64 { .. }
        ))
    ));
}
