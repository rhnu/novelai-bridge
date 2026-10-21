use super::*;

#[test]
fn transport_is_dyn_compatible() {
    fn accepts_dyn(_: &dyn Transport) {}
    struct Ordinary;
    #[async_trait]
    impl Transport for Ordinary {
        async fn execute(&self, _: TransportRequest) -> Result<TransportResponse, TransportError> {
            Ok(TransportResponse::from_bytes(204, Vec::new(), Vec::new()))
        }
    }
    accepts_dyn(&Ordinary);
}

#[test]
fn request_debug_does_not_render_payload_bytes() {
    let mut request = crate::protocol::ApiEndpoint::GenerateImage
        .request("https://example.test", Some(b"private prompt".to_vec()));
    request.endpoint.push_str("?prompt=private-query");
    let debug = format!("{request:?}");
    assert!(!debug.contains("private prompt"));
    assert!(!debug.contains("private-query"));
    assert!(!debug.contains("112, 114, 105"));
    assert!(debug.contains("body_bytes"));
}
