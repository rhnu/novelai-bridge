use base64::Engine;
use serde_json::json;

use super::decode_event;
use crate::{BridgeError, ImageStreamError, ImageStreamEvent, TransportSseEvent};

fn raw(event: &str, value: &serde_json::Value) -> TransportSseEvent {
    TransportSseEvent {
        event: event.into(),
        id: "provider-event-id".into(),
        data: value.to_string(),
        retry: Some(std::time::Duration::from_secs(2)),
    }
}

#[test]
fn official_preview_retains_its_type_and_complete_source() {
    let raw = raw(
        "intermediate",
        &json!({"event_type":"intermediate","samp_ix":0,"step_ix":2,"gen_id":3,"image":"cHJldmlldw==","sigma":0.5,"extension":{"future":[1,2]}}),
    );
    let chunk = decode_event(raw.clone()).unwrap();
    assert!(matches!(
        chunk.event,
        ImageStreamEvent::Intermediate {
            samp_ix: 0,
            step_ix: 2,
            gen_id: Some(3),
            sigma: Some(0.5),
            ..
        }
    ));
    assert_eq!(chunk.raw.as_sse(), Some(&raw));
}

#[test]
fn official_final_is_a_complete_validated_image_without_preview_fields() {
    let image = crate::generation::test_support::image();
    let raw = raw(
        "final",
        &json!({"event_type":"final","samp_ix":1,"gen_id":3,"image":image.to_base64(),"extension":true}),
    );
    let chunk = decode_event(raw.clone()).unwrap();
    let ImageStreamEvent::Final {
        samp_ix,
        image: parsed,
        ..
    } = chunk.event
    else {
        panic!("expected final")
    };
    assert_eq!(samp_ix, 1);
    assert_eq!(parsed.bytes(), image.as_bytes());
    assert_eq!(chunk.raw.as_sse(), Some(&raw));
}

#[test]
fn provider_errors_preserve_message_source_and_optional_sample() {
    for sample in [None, Some(1)] {
        let mut value = json!({"event_type":"error","message":"provider failure","details":{"retryable":false},"gen_id":3,"image":""});
        if let Some(sample) = sample {
            value["samp_ix"] = json!(sample);
        }
        let raw = raw("error", &value);
        let Err(BridgeError::Stream(ImageStreamError::Provider { event })) =
            decode_event(raw.clone())
        else {
            panic!("expected provider error")
        };
        assert!(
            matches!(&event.event, ImageStreamEvent::Error { samp_ix, message, .. } if *samp_ix == sample && message == "provider failure")
        );
        assert_eq!(event.raw.as_sse(), Some(&raw));
        assert!(!format!("{event:?}").contains("provider failure"));
    }
}

#[test]
fn unknown_extensions_preserve_arbitrary_shapes_and_non_json() {
    for raw in [
        raw(
            "",
            &json!({"event_type":"future-event","new_object":{"items":[1,true,null]}}),
        ),
        raw("future-event", &json!([1, 2, 3])),
        raw("future-event", &json!({"event_type":{"version":2}})),
        raw(
            "future-event",
            &json!({"event_type":"nested-extension-kind"}),
        ),
        TransportSseEvent {
            event: "future-binary".into(),
            data: "opaque future format".into(),
            ..Default::default()
        },
    ] {
        let chunk = decode_event(raw.clone()).unwrap();
        assert!(matches!(chunk.event, ImageStreamEvent::Unknown { .. }));
        assert_eq!(chunk.raw.as_sse(), Some(&raw));
    }
}

#[test]
fn known_events_never_fall_back_to_unknown_on_malformed_data() {
    for raw in [
        raw("final", &json!({"event_type":"intermediate"})),
        raw(
            "future-event",
            &json!({"event_type":"error","message":"must not be hidden"}),
        ),
        raw("final", &json!({"samp_ix":0})),
        raw("error", &json!({"message":42})),
        raw(
            "intermediate",
            &json!({"samp_ix":0,"step_ix":1,"gen_id":1,"image":42}),
        ),
        raw("", &json!({"event_type":42})),
        TransportSseEvent {
            event: "final".into(),
            data: "not JSON".into(),
            ..Default::default()
        },
    ] {
        let Err(BridgeError::Stream(ImageStreamError::InvalidEvent { raw: retained, .. })) =
            decode_event(raw.clone())
        else {
            panic!("expected invalid event")
        };
        assert_eq!(retained.as_sse(), Some(&raw));
    }
}

#[test]
fn explicit_sse_name_also_recognizes_official_events_without_json_tag() {
    let chunk = decode_event(raw(
        "intermediate",
        &json!({"samp_ix":0,"step_ix":1,"gen_id":1,"image":"cHJldmlldw=="}),
    ))
    .unwrap();
    assert!(matches!(chunk.event, ImageStreamEvent::Intermediate { .. }));
}

#[test]
fn corrupt_final_images_fail_with_original_data_and_cause() {
    let mut corrupt_png = crate::generation::test_support::image().as_bytes().to_vec();
    corrupt_png.truncate(corrupt_png.len() - 4);
    for image in [
        "not base64!".to_owned(),
        base64::engine::general_purpose::STANDARD.encode(b"not an image"),
        base64::engine::general_purpose::STANDARD.encode(corrupt_png),
    ] {
        let raw = raw("final", &json!({"samp_ix":0,"gen_id":1,"image":image}));
        let Err(BridgeError::Stream(ImageStreamError::InvalidFinal {
            samp_ix,
            raw: retained,
            source,
        })) = decode_event(raw.clone())
        else {
            panic!("expected invalid final")
        };
        assert_eq!(samp_ix, 0);
        assert_eq!(retained.as_sse(), Some(&raw));
        assert_ne!(source.to_string(), "");
    }
}
