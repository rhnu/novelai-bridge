use crate::{ImageStreamChunk, ImageStreamEventType};

#[test]
fn stream_event_identity_preserves_unknown_provider_types() {
    for (identifier, expected) in [
        ("step", ImageStreamEventType::Step),
        (
            "future-event",
            ImageStreamEventType::Unknown("future-event".to_owned()),
        ),
    ] {
        let encoded = serde_json::to_value(identifier).unwrap();
        let parsed: ImageStreamEventType = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), encoded);
    }
}

#[test]
fn stream_chunk_debug_omits_image_content() {
    let chunk = ImageStreamChunk {
        event_type: ImageStreamEventType::Step,
        samp_ix: 1,
        step_ix: Some(3),
        gen_id: 2,
        sigma: None,
        image: "private-payload-marker".to_owned(),
    };
    assert!(!format!("{chunk:?}").contains("private-payload-marker"));
}
