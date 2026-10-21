use super::*;

#[test]
fn base64_budget_accounts_for_padding() {
    let policy = ImageDecodePolicy {
        max_encoded_bytes: 3,
        ..Default::default()
    };
    assert_eq!(
        check_base64_budget(16, 0, policy).unwrap_err().limit,
        ImageLimit::EncodedBytes
    );
    assert!(check_base64_budget(4, 0, policy).is_ok());
    let one_byte = ImageDecodePolicy {
        max_encoded_bytes: 1,
        ..policy
    };
    assert!(check_base64_budget(4, 2, one_byte).is_ok());
}
