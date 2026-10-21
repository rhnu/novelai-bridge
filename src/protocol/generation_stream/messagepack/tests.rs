use serde::Serialize;
use serde_json::json;

use super::decode_event;
use crate::{BridgeError, ImageStreamError, ImageStreamEvent, StreamEventSource};

fn encoded(value: &serde_json::Value) -> Vec<u8> { rmp_serde::to_vec_named(value).unwrap() }

struct Binary<'a>(&'a [u8]);

impl Serialize for Binary<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

#[test]
fn binary_and_byte_array_images_share_events_without_invented_generation_ids() {
    #[derive(Serialize)]
    struct FramePayload<'a> {
        event_type: &'a str,
        samp_ix: u32,
        image: Binary<'a>,
    }
    let image = crate::generation::test_support::image();
    for payload in [
        rmp_serde::to_vec_named(&FramePayload {
            event_type: "final",
            samp_ix: 1,
            image: Binary(image.as_bytes()),
        })
        .unwrap(),
        encoded(
            &json!({"event_type":"final", "samp_ix":1, "image":image.as_bytes(), "future":{"x":[1,true]}}),
        ),
    ] {
        let chunk = decode_event(payload.clone()).unwrap();
        assert!(
            matches!(&chunk.event, ImageStreamEvent::Final { samp_ix:1, gen_id:None, image:received }
            if received.bytes() == image.as_bytes())
        );
        assert_eq!(chunk.raw, StreamEventSource::MessagePack(payload));
    }
    let payload = encoded(&json!({"event_type":"intermediate", "samp_ix":0,
        "step_ix":2, "image":[0, 128, 255], "gen_id":5, "sigma":0.5}));
    let chunk = decode_event(payload.clone()).unwrap();
    assert!(matches!(&chunk.event, ImageStreamEvent::Intermediate {
        samp_ix:0, step_ix:2, gen_id:Some(5), sigma:Some(sigma), image
    } if (*sigma - 0.5).abs() < f32::EPSILON && image == &[0,128,255]));
    assert_eq!(chunk.raw.as_messagepack(), Some(payload.as_slice()));
}

#[test]
fn provider_errors_and_unknown_extensions_keep_original_payloads() {
    let payload = encoded(
        &json!({"event_type":"error", "message":"private failure", "samp_ix":1,
        "future":{"retryable":false}}),
    );
    let Err(BridgeError::Stream(ImageStreamError::Provider { event })) =
        decode_event(payload.clone())
    else {
        panic!("expected provider error");
    };
    assert!(matches!(&event.event, ImageStreamEvent::Error {
        samp_ix:Some(1), gen_id:None, message
    } if message == "private failure"));
    assert_eq!(event.raw.as_messagepack(), Some(payload.as_slice()));
    assert!(!format!("{event:?}").contains("private failure"));
    let payload = encoded(
        &json!({"event_type":"extension", "image":{"not":"image bytes"}, "samp_ix":"opaque"}),
    );
    let chunk = decode_event(payload.clone()).unwrap();
    assert!(
        matches!(chunk.event, ImageStreamEvent::Unknown { event_type } if event_type == "extension")
    );
    assert_eq!(chunk.raw.as_messagepack(), Some(payload.as_slice()));
}

#[test]
fn binary_blobs_are_not_accepted_as_discriminators_messages_or_map_keys() {
    #[derive(Serialize)]
    struct BinaryIdentity<'a> {
        event_type: Binary<'a>,
        samp_ix: u32,
        image: Binary<'a>,
    }
    #[derive(Serialize)]
    struct BinaryMessage<'a> {
        event_type: &'a str,
        message: Binary<'a>,
    }
    let image = crate::generation::test_support::image();
    let discriminator = rmp_serde::to_vec_named(&BinaryIdentity {
        event_type: Binary(b"final"),
        samp_ix: 0,
        image: Binary(image.as_bytes()),
    })
    .unwrap();
    let message = rmp_serde::to_vec_named(&BinaryMessage {
        event_type: "error",
        message: Binary(b"not a string"),
    })
    .unwrap();
    let mut key = vec![0x81, 0xC4, 10];
    key.extend(b"event_type");
    key.push(0xA9);
    key.extend(b"extension");
    for payload in [discriminator, message, key] {
        let Err(BridgeError::Stream(ImageStreamError::InvalidEvent { raw, .. })) =
            decode_event(payload.clone())
        else {
            panic!("binary text must fail strict typing");
        };
        assert_eq!(raw.as_messagepack(), Some(payload.as_slice()));
    }
}

#[test]
fn malformed_known_events_never_become_unknown_and_do_not_accept_text_images() {
    for value in [
        json!({"event_type":42}),
        json!({"image":[1]}),
        json!({"event_type":"intermediate", "samp_ix":0, "image":[1]}),
        json!({"event_type":"intermediate", "samp_ix":0, "step_ix":1, "image":[256]}),
        json!({"event_type":"final", "samp_ix":-1, "image":[1]}),
        json!({"event_type":"final", "samp_ix":0, "image":"base64 is not binary bytes"}),
        json!({"event_type":"error", "message":42}),
    ] {
        let payload = encoded(&value);
        let Err(BridgeError::Stream(ImageStreamError::InvalidEvent { raw, .. })) =
            decode_event(payload.clone())
        else {
            panic!("expected malformed known event: {value}");
        };
        assert_eq!(raw.as_messagepack(), Some(payload.as_slice()));
    }
}

#[test]
fn invalid_final_images_remain_invalid_finals_with_the_complete_source() {
    let mut corrupt = crate::generation::test_support::image().as_bytes().to_vec();
    corrupt.truncate(corrupt.len() - 4);
    for bytes in [b"not an image".to_vec(), corrupt] {
        let payload = encoded(&json!({"event_type":"final", "samp_ix":0, "image":bytes}));
        let Err(BridgeError::Stream(ImageStreamError::InvalidFinal {
            samp_ix: 0,
            raw,
            source,
        })) = decode_event(payload.clone())
        else {
            panic!("expected invalid final");
        };
        assert_eq!(raw.as_messagepack(), Some(payload.as_slice()));
        assert_ne!(source.to_string(), "");
    }
}

#[test]
fn trailing_values_nonmaps_forged_container_lengths_and_excessive_nesting_fail() {
    let valid = encoded(&json!({"event_type":"extension"}));
    let mut trailing = valid;
    trailing.push(0xC0);
    let mut deep = vec![0x82, 0xAA];
    deep.extend(b"event_type");
    deep.extend([0xA9]);
    deep.extend(b"extension");
    deep.extend([0xA1, b'x']);
    deep.extend(std::iter::repeat_n(0x91, 70));
    deep.push(0xC0);
    // Tiny frames declare enormous array/bin lengths; decoding must not reserve them.
    let mut array = vec![0x81, 0xA1, b'x', 0xDD, 0xFF, 0xFF, 0xFF, 0xFF];
    let mut binary = array.clone();
    binary[3] = 0xC6;
    let mut duplicate = vec![0x82, 0xAA];
    duplicate.extend(b"event_type");
    duplicate.extend([0xA9]);
    duplicate.extend(b"extension");
    duplicate.extend([0xAA]);
    duplicate.extend(b"event_type");
    duplicate.extend([0xA9]);
    duplicate.extend(b"extension");
    for payload in [
        trailing,
        deep,
        std::mem::take(&mut array),
        binary,
        duplicate,
        vec![0x90],
        vec![0xC0],
    ] {
        assert!(matches!(
            decode_event(payload),
            Err(BridgeError::Stream(ImageStreamError::InvalidEvent { .. }))
        ));
    }
}
