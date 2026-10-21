use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;
use futures_util::{StreamExt, stream};

use super::{GenerateImageStream, ImageStreamResult};
use crate::{
    CreditEvent, CreditObserver, DecodeError, DecodeTarget, ImageStreamChunk, Model,
    ObservedOperation, RequestCostEstimate, RequestOutcome, client::observe::ObservationContext,
};

#[derive(Default)]
struct Observer {
    events: Mutex<Vec<CreditEvent>>,
    dropped: Option<Arc<AtomicBool>>,
}

#[async_trait]
impl CreditObserver for Observer {
    async fn observe(&self, event: CreditEvent) {
        if matches!(
            event,
            CreditEvent::RequestSettled {
                outcome: RequestOutcome::Cancelled,
                ..
            }
        ) && let Some(dropped) = &self.dropped
        {
            assert!(dropped.load(Ordering::Relaxed));
        }
        self.events
            .lock()
            .expect("test event mutex is not poisoned")
            .push(event);
    }
}

async fn observed(input: ImageStreamResult, observer: Arc<Observer>) -> GenerateImageStream {
    let pending = ObservationContext::new(observer)
        .begin(
            ObservedOperation::EncodeVibe {
                model: Model::NaiDiffusion45Full,
            },
            RequestCostEstimate::Fixed { anlas: 2 },
        )
        .await;
    GenerateImageStream::new(10, input, pending)
}

fn chunk(sample: u32) -> ImageStreamChunk {
    ImageStreamChunk {
        event_type: "step".to_owned(),
        samp_ix: sample,
        step_ix: Some(1),
        gen_id: 1,
        sigma: None,
        image: "fragment".to_owned(),
    }
}

fn outcomes(observer: &Observer) -> Vec<RequestOutcome> {
    observer
        .events
        .lock()
        .expect("test event mutex is not poisoned")
        .iter()
        .filter_map(|event| match event {
            CreditEvent::RequestSettled { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn eof_settles_once_with_distinct_samples_even_after_into_inner() {
    let observer = Arc::new(Observer::default());
    let input = Box::pin(stream::iter([Ok(chunk(0)), Ok(chunk(0)), Ok(chunk(1))]));
    let mut output = observed(input, Arc::clone(&observer)).await.into_inner();
    assert_eq!(output.by_ref().collect::<Vec<_>>().await.len(), 3);
    assert!(output.next().await.is_none());
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Succeeded { image_count: 2 }]
    );
}

#[tokio::test]
async fn an_error_stops_consumption_and_never_reports_success() {
    let observer = Arc::new(Observer::default());
    let error = DecodeError::new(DecodeTarget::StreamChunk, "invalid stream".into());
    let input = Box::pin(stream::iter([Err(error.into()), Ok(chunk(0))]));
    let output = observed(input, Arc::clone(&observer))
        .await
        .collect::<Vec<_>>()
        .await;
    assert_eq!(output.len(), 1);
    assert!(output[0].is_err());
    assert!(matches!(
        outcomes(&observer).as_slice(),
        [RequestOutcome::Failed { .. }]
    ));
}

#[tokio::test]
async fn empty_eof_has_zero_samples_and_drop_does_not_guess_an_outcome() {
    let observer = Arc::new(Observer::default());
    let output = observed(Box::pin(stream::empty()), Arc::clone(&observer))
        .await
        .collect::<Vec<_>>()
        .await;
    assert!(output.is_empty());
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Succeeded { image_count: 0 }]
    );
    drop(observed(Box::pin(stream::pending()), Arc::clone(&observer)).await);
    assert_eq!(outcomes(&observer).len(), 1);
}

struct DropStream(Arc<AtomicBool>);

impl futures_core::Stream for DropStream {
    type Item = crate::ImageStreamItem;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Pending
    }
}

impl Drop for DropStream {
    fn drop(&mut self) { self.0.store(true, Ordering::Relaxed); }
}

#[tokio::test]
async fn explicit_cancel_drops_the_input_before_observer_notification() {
    let dropped = Arc::new(AtomicBool::new(false));
    let observer = Arc::new(Observer {
        events: Mutex::new(Vec::new()),
        dropped: Some(Arc::clone(&dropped)),
    });
    observed(Box::pin(DropStream(dropped)), Arc::clone(&observer))
        .await
        .cancel()
        .await;
    assert_eq!(outcomes(&observer), [RequestOutcome::Cancelled]);
}
