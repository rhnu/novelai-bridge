use crate::{ImageStreamChunk, ImageStreamEvent, ImageStreamEventType, TransportSseEvent};

#[test]
fn stream_event_identity_only_uses_unknown_for_extensions() {
    for (identifier, expected) in [
        ("intermediate", ImageStreamEventType::Intermediate),
        ("final", ImageStreamEventType::Final),
        ("error", ImageStreamEventType::Error),
        ("step", ImageStreamEventType::Unknown("step".to_owned())),
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
fn stream_debug_omits_images_raw_data_and_provider_messages() {
    for event in [
        ImageStreamEvent::Intermediate {
            samp_ix: 1,
            step_ix: 3,
            gen_id: 2,
            sigma: None,
            image: "private-payload-marker".to_owned(),
        },
        ImageStreamEvent::Error {
            samp_ix: None,
            gen_id: None,
            message: "private-error-marker".to_owned(),
        },
    ] {
        let chunk = ImageStreamChunk {
            event,
            raw: TransportSseEvent {
                id: "private-id-marker".into(),
                data: "private-raw-marker".into(),
                ..Default::default()
            },
        };
        let debug = format!("{chunk:?}");
        for marker in [
            "private-payload-marker",
            "private-error-marker",
            "private-raw-marker",
            "private-id-marker",
        ] {
            assert!(!debug.contains(marker));
        }
    }
}
