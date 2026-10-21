use super::*;

#[test]
fn request_preserves_a_service_prefix_and_assigns_json_headers_to_posts() {
    let request =
        ApiEndpoint::EncodeVibe.request("https://example.test/root/", Some(b"{}".to_vec()));
    assert_eq!(request.endpoint, "https://example.test/root/ai/encode-vibe");
    assert_eq!(request.method, TransportMethod::Post);
    assert_eq!(request.content_type.as_deref(), Some("application/json"));
    assert_eq!(request.body.as_deref(), Some(b"{}".as_slice()));
}

#[test]
fn account_queries_have_no_body_content_type() {
    let request = ApiEndpoint::Subscription.request("https://example.test", None);
    assert_eq!(request.method, TransportMethod::Get);
    assert_eq!(request.accept.as_deref(), Some("application/json"));
    assert!(request.content_type.is_none());
    assert!(request.body.is_none());
}
