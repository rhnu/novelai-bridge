use super::resolve_seed_with;
use crate::{BridgeError, PreparationError};

#[test]
fn random_source_failures_keep_their_execution_classification_and_source() {
    let error: BridgeError = resolve_seed_with(0, |_| Err(getrandom::Error::UNSUPPORTED))
        .unwrap_err()
        .into();
    let BridgeError::Preparation(PreparationError::SeedGenerationFailed { source }) = error else {
        panic!("an unavailable random source is an execution failure");
    };
    assert_eq!(
        source.downcast_ref::<getrandom::Error>(),
        Some(&getrandom::Error::UNSUPPORTED)
    );
}

#[test]
fn automatic_seed_is_nonzero_and_a_fixed_seed_does_not_touch_randomness() {
    let automatic = resolve_seed_with(0, |bytes| {
        bytes.fill(0);
        Ok(())
    })
    .unwrap();
    assert_ne!(automatic, 0);
    assert_eq!(
        resolve_seed_with(42, |_| panic!("fixed seeds never require randomness")).unwrap(),
        42
    );
}
