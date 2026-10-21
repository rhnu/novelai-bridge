use super::{
    ImageStreamChunk, ImageStreamEvent, ImageStreamEventType, StreamEventSource, TransportSseEvent,
};

#[test]
fn stream_event_identity_only_uses_unknown_for_extensions() {
    for (identifier, expected) in [
        ("intermediate", ImageStreamEventType::Intermediate),
        ("final", ImageStreamEventType::Final),
        ("error", ImageStreamEventType::Error),
        ("step", ImageStreamEventType::Unknown("step".to_owned())),
    ] {
        let encoded = serde_json::to_value(identifier).unwrap();
        let parsed: ImageStreamEventType = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(serde_json::to_value(&parsed).unwrap(), encoded);
    }
}

#[test]
fn debug_omits_protocol_payloads_and_provider_messages() {
    for raw in [
        StreamEventSource::Sse(TransportSseEvent {
            id: "private-id-marker".into(),
            data: "private-raw-marker".into(),
            ..Default::default()
        }),
        StreamEventSource::MessagePack(b"private-raw-marker".to_vec()),
    ] {
        for event in [
            ImageStreamEvent::Intermediate {
                samp_ix: 1,
                step_ix: 3,
                gen_id: None,
                sigma: None,
                image: b"private-payload-marker".to_vec(),
            },
            ImageStreamEvent::Error {
                samp_ix: None,
                gen_id: None,
                message: "private-error-marker".into(),
            },
        ] {
            let debug = format!(
                "{:?}",
                ImageStreamChunk {
                    event,
                    raw: raw.clone()
                }
            );
            for marker in [
                "private-id-marker",
                "private-raw-marker",
                "private-payload-marker",
                "private-error-marker",
            ] {
                assert!(!debug.contains(marker));
            }
        }
    }
}

#[test]
fn source_accessors_do_not_convert_or_fabricate_another_protocol() {
    let sse = TransportSseEvent {
        event: "extension".into(),
        ..Default::default()
    };
    let source: StreamEventSource = sse.clone().into();
    assert_eq!(source.as_sse(), Some(&sse));
    assert!(source.as_messagepack().is_none());
    let payload = vec![0x81, 0xA1, b'x', 1];
    let source = StreamEventSource::MessagePack(payload.clone());
    assert_eq!(source.as_messagepack(), Some(payload.as_slice()));
    assert!(source.as_sse().is_none());
}
