use serde_json::json;

use super::{CacheState, hash_media};
use crate::image::request_body::{Media, MediaBody, MediaSlot};

fn body() -> MediaBody {
    MediaBody::new(
        &json!({"parameters": {"image": ""}}),
        vec![MediaSlot {
            pointer: "/parameters/image".to_owned(),
            data: Media::Image(vec![1, 2, 3].into()),
            omitted: false,
        }],
    )
    .unwrap()
}

#[test]
fn prehashed_media_uses_registration_changes_before_projection() {
    let mut state = CacheState::default();
    let secret = state.secret().unwrap();
    let pending = body();
    let keys = hash_media(&secret, &pending);
    let registered = keys.iter().map(|(_, key)| key.clone()).collect::<Vec<_>>();
    state.register(registered.clone(), 1);
    let cached = state.transform(pending, keys);
    assert!(cached.body.media[0].omitted);
    assert_eq!(cached.registrations, [] as [String; 0]);

    let pending = body();
    let keys = hash_media(&secret, &pending);
    state.invalidate(&registered);
    let restored = state.transform(pending, keys);
    assert!(!restored.body.media[0].omitted);
    assert_eq!(restored.registrations, registered);
    assert_eq!(state.secret().unwrap(), secret);
}
