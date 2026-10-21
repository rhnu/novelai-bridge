use futures_util::StreamExt;
use novelai_bridge::{
    GenerateImageStreamRequest, ImageStreamChunk, ImageStreamEvent, ImageStreamEventType,
    TransportSseEvent,
};

use super::*;

#[tokio::test]
async fn generate_stream_parses_chunk() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![ImageStreamChunk {
        event: ImageStreamEvent::Intermediate { samp_ix: 0, step_ix: 1, gen_id: 1, sigma: Some(1.1), image: "abc".into() },
        raw: TransportSseEvent { event: "intermediate".into(), data: r#"{"event_type":"intermediate","samp_ix":0,"step_ix":1,"gen_id":1,"sigma":1.1,"image":"abc"}"#.into(), ..Default::default() },
    }]);
    let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());

    let params = GenerateImageStreamRequest {
        base: request("1girl"),
        ..GenerateImageStreamRequest::default()
    };

    let mut stream = client.generate_stream(params).await.unwrap();
    assert_ne!(stream.resolved_seed, 0);
    assert_eq!(
        stream.resolved_seed,
        transport.last_body().unwrap()["parameters"]["seed"]
            .as_i64()
            .unwrap()
    );
    let item = stream.next().await.unwrap().unwrap();
    assert_eq!(item.event_type(), ImageStreamEventType::Intermediate);
    assert!(matches!(
        item.event,
        ImageStreamEvent::Intermediate { step_ix: 1, .. }
    ));
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.novelai.net/ai/generate-image-stream")
    );
}

#[tokio::test]
async fn generate_stream_uses_custom_image_base_url() {
    let transport = MockTransport::default();
    transport.set_sse_chunks(vec![]);
    let client = Client::new(
        Arc::new(transport.clone()),
        ClientConfig {
            image_base_url: "https://image.example.test/root/".to_owned(),
        },
    );

    let params = GenerateImageStreamRequest {
        base: request("1girl"),
        ..GenerateImageStreamRequest::default()
    };

    let _stream = client.generate_stream(params).await.unwrap();
    assert_eq!(
        transport.last_path().as_deref(),
        Some("https://image.example.test/root/ai/generate-image-stream")
    );
}

#[tokio::test]
async fn generate_stream_rejects_v3_models_before_transport() {
    for model in [Model::NaiDiffusion3, Model::NaiDiffusion3Furry] {
        let transport = MockTransport::default();
        let client = Client::new(Arc::new(transport.clone()), ClientConfig::default());
        let params = GenerateImageStreamRequest {
            base: GenerateImageRequest::builder("1girl", model).build(),
            ..GenerateImageStreamRequest::default()
        };

        let Err(error) = client.generate_stream(params).await else {
            panic!("V3 models must not start a stream");
        };
        assert!(matches!(
            error,
            BridgeError::InvalidRequest(InvalidRequest::UnsupportedModelFeature { ref feature, .. })
                if *feature == RequestField::StreamDelivery
        ));
        assert!(transport.last_path().is_none());
    }
}
