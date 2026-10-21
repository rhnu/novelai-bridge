use super::*;

#[test]
fn base64_budget_counts_utf8_bytes_before_invalid_input_can_allocate() {
    let policy = ImageDecodePolicy {
        max_encoded_bytes: 3,
        ..Default::default()
    };
    assert_eq!(
        check_base64_budget("🙂🙂🙂🙂", policy).unwrap_err().limit,
        ImageLimit::EncodedBytes
    );
    assert!(check_base64_budget("QUJD", policy).is_ok());
    let one_byte = ImageDecodePolicy {
        max_encoded_bytes: 1,
        ..policy
    };
    assert!(check_base64_budget("Q Q = =", one_byte).is_ok());
}
