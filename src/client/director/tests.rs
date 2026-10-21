use std::{
    io::{Cursor, Write},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use crate::{
    Client, ClientConfig, CreditEvent, CreditObserver, DirectorCommand, EncodedImage,
    RequestOutcome, RunDirectorToolRequest, Transport, TransportError, TransportRequest,
    TransportResponse, ValidationPolicy,
};

struct ImageResponse(Vec<u8>);

#[async_trait]
impl Transport for ImageResponse {
    async fn execute(
        &self,
        _request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        Ok(TransportResponse::from_bytes(
            200,
            Vec::new(),
            self.0.clone(),
        ))
    }
}

#[derive(Default)]
struct Observer(Mutex<Vec<CreditEvent>>);

#[async_trait]
impl CreditObserver for Observer {
    async fn observe(&self, event: CreditEvent) { self.0.lock().unwrap().push(event); }
}

#[tokio::test]
async fn director_raw_and_prepared_calls_keep_all_images_and_observe_their_count() {
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in ["first.png", "second.png", "third.png"] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&png).unwrap();
    }
    let payload = writer.finish().unwrap().into_inner();
    let observer = Arc::new(Observer::default());
    let client = Client::new(Arc::new(ImageResponse(payload)), ClientConfig::default())
        .with_credit_observer(Arc::<Observer>::clone(&observer));
    let request = RunDirectorToolRequest {
        image: EncodedImage::from_bytes(png).unwrap(),
        command: DirectorCommand::Sketch,
        policy: ValidationPolicy::Reject,
    };
    let prepared = request.clone().prepare().unwrap();
    let ordinary = client.run_director_tool(request).await.unwrap();
    let reused = client.run_director_tool_prepared(prepared).await.unwrap();
    assert_eq!(ordinary.images.len(), 3);
    assert_eq!(ordinary, reused);

    let outcomes = observer
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| match event {
            CreditEvent::RequestSettled { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        vec![
            RequestOutcome::Succeeded { image_count: 3 },
            RequestOutcome::Succeeded { image_count: 3 },
        ]
    );
}
