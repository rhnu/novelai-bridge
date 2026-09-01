#![cfg(all(feature = "client", feature = "streaming"))]

//! Valid wire payloads captured at 8fd7675. Changes require deliberate fixture review.
mod support;

use novelai_bridge::{Client, GenerateImageRequest, GenerateImageStreamRequest, Model, StreamMode};
use serde_json::{Map, Value};

#[tokio::test]
async fn valid_generation_payloads_match_reviewed_fixtures() {
    let transport = support::MockTransport::default();
    let client = Client::with_transport(transport.clone());
    let mut actual = Map::new();
    for model in [
        Model::NaiDiffusion3,
        Model::NaiDiffusion3Furry,
        Model::NaiDiffusion4Full,
        Model::NaiDiffusion4Curated,
        Model::NaiDiffusion45Full,
        Model::NaiDiffusion45Curated,
        Model::NaiDiffusion5Full,
        Model::NaiDiffusion5Curated,
    ] {
        let request = GenerateImageRequest::builder("1girl", model)
            .seed(42)
            .build();
        client.generate(request.clone()).await.unwrap();
        actual.insert(
            format!("{}/generate", model.as_str()),
            transport.last_body().unwrap(),
        );
        if model.descriptor().support().streaming {
            let _stream = client
                .generate_stream(GenerateImageStreamRequest {
                    base: request,
                    stream: StreamMode::default(),
                })
                .await
                .unwrap();
            actual.insert(
                format!("{}/stream", model.as_str()),
                transport.last_body().unwrap(),
            );
        }
    }
    assert_eq!(
        serde_json::to_string_pretty(&Value::Object(actual)).unwrap(),
        include_str!("support/wire/generation.json").trim(),
    );
}
