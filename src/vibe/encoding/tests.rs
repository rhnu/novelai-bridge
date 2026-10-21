use super::*;

#[test]
fn vibe_encoding_is_opaque_nonempty_data() {
    let encoding = VibeEncoding::from_bytes(vec![1, 2, 3]).unwrap();
    assert_eq!(encoding.to_base64(), "AQID");
    assert_eq!(VibeEncoding::from_base64(" A Q I D \n").unwrap(), encoding);
    assert!(matches!(
        VibeEncoding::from_bytes(Vec::new()),
        Err(BridgeError::InvalidRequest(
            InvalidRequest::EmptyField { .. }
        ))
    ));
}

#[test]
fn malformed_and_empty_base64_are_rejected_at_the_encoding_boundary() {
    assert!(matches!(
        VibeEncoding::from_base64("not base64!"),
        Err(BridgeError::InvalidRequest(
            InvalidRequest::InvalidBase64 { .. }
        ))
    ));
    assert!(matches!(
        VibeEncoding::from_base64("  \n"),
        Err(BridgeError::InvalidRequest(
            InvalidRequest::EmptyField { .. }
        ))
    ));
}
