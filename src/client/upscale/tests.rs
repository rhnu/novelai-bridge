use std::{
    io::{Cursor, Write},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use crate::{
    Client, ClientConfig, CreditEvent, CreditObserver, EncodedImage, RequestCostEstimate,
    RequestOutcome, Transport, TransportError, TransportMethod, TransportRequest,
    TransportResponse, UpscaleImageRequest,
};

struct Backend {
    response: Vec<u8>,
    requests: Mutex<Vec<TransportRequest>>,
}

#[async_trait]
impl Transport for Backend {
    async fn execute(
        &self,
        request: TransportRequest,
    ) -> Result<TransportResponse, TransportError> {
        self.requests.lock().unwrap().push(request);
        Ok(TransportResponse::from_bytes(
            200,
            Vec::new(),
            self.response.clone(),
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
async fn upscale_raw_and_prepared_calls_preserve_all_images_and_observe_their_count() {
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in ["first.png", "second.png"] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&png).unwrap();
    }
    let backend = Arc::new(Backend {
        response: zip.finish().unwrap().into_inner(),
        requests: Mutex::default(),
    });
    let observer = Arc::new(Observer::default());
    let client = Client::new(Arc::<Backend>::clone(&backend), ClientConfig::default())
        .with_credit_observer(Arc::<Observer>::clone(&observer));
    let request = UpscaleImageRequest::new(EncodedImage::from_bytes(png).unwrap());
    let prepared = request.clone().prepare().unwrap();
    let raw = client.upscale(request).await.unwrap();
    let reused = client.upscale_prepared(prepared).await.unwrap();
    assert_eq!(raw.images.len(), 2);
    assert_eq!(raw, reused);
    let requests = backend.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[0].method, TransportMethod::Post);
    assert!(requests[0].endpoint.ends_with("/ai/upscale"));
    drop(requests);
    let events = observer.0.lock().unwrap().clone();
    assert_eq!(events.len(), 4);
    assert!(matches!(
        &events[0],
        CreditEvent::RequestPlanned {
            estimate: RequestCostEstimate::Fixed { anlas: 1 },
            ..
        }
    ));
    assert!(matches!(
        &events[1],
        CreditEvent::RequestSettled {
            outcome: RequestOutcome::Succeeded { image_count: 2 },
            ..
        }
    ));
}
