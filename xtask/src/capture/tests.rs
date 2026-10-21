use novelai_bridge::{ApiEndpoint, DEFAULT_IMAGE_BASE};

use super::*;

#[test]
fn body_edits_cannot_silently_succeed_for_a_bodyless_operation() {
    let edits = RequestEdits::new(&["field=true".to_owned()], vec![]).unwrap();
    assert!(
        edits
            .apply(ApiEndpoint::Subscription.request(DEFAULT_IMAGE_BASE, None))
            .is_err()
    );
    let empty = RequestEdits::new(&[], vec![]).unwrap();
    assert!(
        empty
            .apply(ApiEndpoint::Subscription.request(DEFAULT_IMAGE_BASE, None))
            .is_ok()
    );
}
