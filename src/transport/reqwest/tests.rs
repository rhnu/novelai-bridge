#[cfg(feature = "reqwest-streaming")]
#[test]
fn event_stream_media_type_is_checked_before_framing() {
    let mut headers = reqwest::header::HeaderMap::new();
    assert!(!super::is_event_stream(&headers));
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    assert!(!super::is_event_stream(&headers));
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        "Text/Event-Stream; charset=utf-8".parse().unwrap(),
    );
    assert!(super::is_event_stream(&headers));
}

#[cfg(feature = "reqwest-streaming")]
#[tokio::test]
async fn sse_framing_preserves_event_id_retry_and_empty_extension_data() {
    use eventsource_stream::Eventsource;
    use futures_util::{StreamExt, stream};

    let input = stream::iter([Ok::<_, std::io::Error>(
        b"event: intermediate\nid: sample-1\nretry: 2500\ndata: {\"samp_ix\":0}\n\nevent: future-event\ndata:\n\n".as_slice()
    )]);
    let mut events = input.eventsource();
    let first = crate::TransportSseEvent::from(events.next().await.unwrap().unwrap());
    assert_eq!(first.event, "intermediate");
    assert_eq!(first.id, "sample-1");
    assert_eq!(first.retry, Some(std::time::Duration::from_millis(2500)));
    assert_eq!(first.data, r#"{"samp_ix":0}"#);
    let second = crate::TransportSseEvent::from(events.next().await.unwrap().unwrap());
    assert_eq!(second.event, "future-event");
    assert_eq!(second.data, "");
}
