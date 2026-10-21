use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::{
    Client, ClientConfig, Model, SuggestTagsRequest, TagLanguage, Transport, TransportError,
    TransportHeader, TransportMethod, TransportRequest, TransportResponse,
};

struct Backend(Mutex<Option<TransportRequest>>);

#[async_trait]
impl Transport for Backend {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        *self.0.lock().unwrap() = Some(request);
        Ok(TransportResponse::from_bytes(
            200,
            vec![TransportHeader {
                name: "content-type".to_owned(),
                value: b"text/plain; charset=utf-8".to_vec(),
            }],
            r#"[{"jp_tag":"猫","en_tag":"cat","power":10000}]"#.as_bytes().to_vec(),
        ))
    }
}

#[tokio::test]
async fn tag_query_uses_get_encoded_parameters_and_accepts_json_under_text_plain() {
    let backend = Arc::new(Backend(Mutex::new(None)));
    let client = Client::new(
        Arc::<Backend>::clone(&backend),
        ClientConfig {
            image_base_url: "https://example.test/root/".to_owned(),
            ..ClientConfig::default()
        },
    );
    let mut query = SuggestTagsRequest::new("猫 &key=value", Model::NaiDiffusion5Full);
    query.language = TagLanguage::Japanese;
    let result = client.suggest_tags(query).await.unwrap();
    assert_eq!(result.len(), 1);
    let crate::SuggestTagsResult::Japanese(tags) = result else {
        panic!("expected the live JP response shape");
    };
    assert_eq!(tags[0].jp_tag, "猫");
    assert_eq!(tags[0].en_tag, "cat");
    assert_eq!(tags[0].power, 10000);
    let request = backend.0.lock().unwrap().take().unwrap();
    assert_eq!(request.method, TransportMethod::Get);
    assert!(request.body.is_none());
    assert!(request.content_type.is_none());
    assert_eq!(
        request.endpoint,
        "https://example.test/root/ai/generate-image/suggest-tags?model=nai-diffusion-5-full&prompt=%E7%8C%AB+%26key%3Dvalue&lang=jp"
    );
}
