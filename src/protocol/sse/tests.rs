use std::time::Duration;

use futures_util::{StreamExt, stream};

use super::decode;
use crate::{BridgeError, TransportByteStream};

#[expect(
    clippy::needless_collect,
    reason = "Own borrowed test iterators before placing them in the static transport stream"
)]
fn input(chunks: impl IntoIterator<Item = Vec<u8>>) -> TransportByteStream {
    Box::pin(stream::iter(chunks.into_iter().map(Ok).collect::<Vec<_>>()))
}

#[tokio::test]
async fn bom_unicode_and_crlf_preserve_event_fields_at_every_split() {
    let bytes = "\u{feff}: comment\r\nevent: extension\r\nid: first\r\nretry: 42\r\ndata: \u{1f63a}\r\ndata: second\r\n\r\ndata: next\r\n\r\n".as_bytes();
    for split in 0..=bytes.len() {
        let mut events = decode(
            input([bytes[..split].to_vec(), bytes[split..].to_vec()]),
            256,
        );
        let first = events.next().await.unwrap().unwrap();
        assert_eq!(first.event, "extension");
        assert_eq!(first.id, "first");
        assert_eq!(first.retry, Some(Duration::from_millis(42)));
        assert_eq!(first.data, "\u{1f63a}\nsecond");
        let second = events.next().await.unwrap().unwrap();
        assert_eq!(second.event, "message");
        assert_eq!(second.id, "first");
        assert_eq!(second.retry, None);
        assert_eq!(second.data, "next");
        assert!(events.next().await.is_none());
    }
}

#[tokio::test]
async fn every_byte_chunks_support_consecutive_split_unicode_characters() {
    let bytes = "data: \u{1f63a}\u{00e9}\u{1f63a}\u{20ac}\u{00e9}\n\n".as_bytes();
    let mut events = decode(input(bytes.iter().map(|byte| vec![*byte])), 128);
    assert_eq!(
        events.next().await.unwrap().unwrap().data,
        "\u{1f63a}\u{00e9}\u{1f63a}\u{20ac}\u{00e9}"
    );
    assert!(events.next().await.is_none());
}

#[tokio::test]
async fn invalid_utf8_fails_before_polling_later_frames() {
    for chunks in [
        vec![b"\xFF\n\n".to_vec()],
        vec![b"data: \xF0".to_vec(), b"\n\n".to_vec()],
    ] {
        let body = stream::iter(chunks.into_iter().map(Ok)).chain(stream::poll_fn(|_| {
            panic!("invalid UTF-8 must fail before polling any later frame")
        }));
        let mut events = decode(Box::pin(body), 16);
        let Err(BridgeError::Decode(error)) = events.next().await.unwrap() else {
            panic!("invalid UTF-8 must be a decoding failure");
        };
        assert!(error.source.downcast_ref::<std::str::Utf8Error>().is_some());
        assert!(events.next().await.is_none());
    }
}

#[tokio::test]
async fn incomplete_unicode_at_eof_fails_and_unterminated_events_are_discarded() {
    let mut truncated = decode(input([b"data: \xF0\x9F".to_vec()]), 32);
    let Err(BridgeError::Decode(error)) = truncated.next().await.unwrap() else {
        panic!("an incomplete UTF-8 character must fail at EOF");
    };
    let source = error.source.downcast_ref::<std::str::Utf8Error>().unwrap();
    assert_eq!(source.error_len(), None);
    assert!(truncated.next().await.is_none());
    for bytes in [b"data: unfinished".as_slice(), b"data: unfinished\n"] {
        let mut events = decode(input([bytes.to_vec()]), 32);
        assert!(events.next().await.is_none());
    }
}

#[tokio::test]
async fn raw_frame_budgets_include_delimiters_and_reset_for_all_line_endings() {
    for bytes in [b"data:x\n\n".as_slice(), b"data:x\r\r", b"data:x\r\n\r\n"] {
        let mut events = decode(
            input(bytes.repeat(3).into_iter().map(|byte| vec![byte])),
            bytes.len(),
        );
        for _ in 0..3 {
            assert_eq!(events.next().await.unwrap().unwrap().data, "x");
        }
        assert!(events.next().await.is_none());
        let mut oversized = decode(input([bytes.to_vec()]), bytes.len() - 1);
        let Err(BridgeError::Decode(error)) = oversized.next().await.unwrap() else {
            panic!("raw framing delimiters must count towards the budget");
        };
        assert!(
            error
                .source
                .downcast_ref::<crate::ResponseLimitExceeded>()
                .is_some()
        );
        assert!(oversized.next().await.is_none());
    }
}

#[tokio::test]
async fn a_fragmented_large_line_and_coalesced_lines_preserve_data() {
    let text = "x".repeat(32 * 1024);
    let bytes = format!("data: {text}\n\n");
    let mut fragmented = decode(
        input(bytes.as_bytes().chunks(31).map(<[u8]>::to_vec)),
        bytes.len(),
    );
    assert_eq!(fragmented.next().await.unwrap().unwrap().data, text);
    assert!(fragmented.next().await.is_none());
    let mut coalesced = decode(
        input([b"data: first\ndata: second\n\ndata: third\n\n".to_vec()]),
        32,
    );
    assert_eq!(
        coalesced.next().await.unwrap().unwrap().data,
        "first\nsecond"
    );
    assert_eq!(coalesced.next().await.unwrap().unwrap().data, "third");
    assert!(coalesced.next().await.is_none());
}
