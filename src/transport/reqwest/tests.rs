#[cfg(feature = "reqwest-streaming")]
#[test]
fn event_stream_media_type_is_checked_before_framing() {
    let mut headers = reqwest::header::HeaderMap::new();
    assert!(!super::is_event_stream(&headers));
    headers.insert(reqwest::header::CONTENT_TYPE, "application/json".parse().unwrap());
    assert!(!super::is_event_stream(&headers));
    headers.insert(reqwest::header::CONTENT_TYPE, "Text/Event-Stream; charset=utf-8".parse().unwrap());
    assert!(super::is_event_stream(&headers));
}
