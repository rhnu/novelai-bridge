use futures_util::{StreamExt, stream};

use super::*;

#[tokio::test]
async fn sse_framing_handles_split_and_multiple_events() {
    let chunks = vec![
        Ok::<Vec<u8>, std::io::Error>(b"data: one\n".to_vec()),
        Ok(b"\ndata: two\n\n".to_vec()),
    ];
    let mut events = stream::iter(chunks).eventsource();
    assert_eq!(events.next().await.unwrap().unwrap().data, "one");
    assert_eq!(events.next().await.unwrap().unwrap().data, "two");
}

#[test]
fn transport_is_dyn_compatible_without_streaming() {
    fn accepts_dyn(_: &dyn Transport) {}
    struct Ordinary;
    #[async_trait]
    impl Transport for Ordinary {
        async fn execute(&self, _: TransportRequest) -> Result<TransportResponse, TransportError> {
            Ok(TransportResponse {
                status: 204,
                headers: Vec::new(),
                body: Vec::new(),
            })
        }
    }
    accepts_dyn(&Ordinary);
}
