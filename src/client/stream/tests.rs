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
            1,
            ObservedOperation::EncodeVibe {
                model: Model::NaiDiffusion45Full,
            },
            RequestCostEstimate::Fixed { anlas: 2 },
        )
        .await;
    GenerateImageStream::new(10, 2, input, pending)
}

fn chunk(sample: u32) -> ImageStreamChunk {
    let image = crate::decode_generated_images(
        crate::generation::test_support::image().as_bytes().to_vec(),
    )
    .expect("generated fixture is a valid PNG")
    .remove(0);
    ImageStreamChunk {
        event: crate::ImageStreamEvent::Final {
            samp_ix: sample,
            gen_id: Some(1),
            image,
        },
        raw: crate::TransportSseEvent::default().into(),
    }
}

fn preview(sample: u32) -> ImageStreamChunk {
    ImageStreamChunk {
        event: crate::ImageStreamEvent::Intermediate {
            samp_ix: sample,
            step_ix: 1,
            gen_id: Some(1),
            sigma: None,
            image: b"preview".to_vec(),
        },
        raw: crate::TransportSseEvent::default().into(),
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
    let input = Box::pin(stream::iter([Ok(preview(0)), Ok(chunk(0)), Ok(chunk(1))]));
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
async fn empty_eof_fails_and_drop_does_not_guess_an_outcome() {
    let observer = Arc::new(Observer::default());
    let output = observed(Box::pin(stream::empty()), Arc::clone(&observer))
        .await
        .collect::<Vec<_>>()
        .await;
    assert!(
        matches!(&output[0], Err(crate::BridgeError::Stream(crate::ImageStreamError::Incomplete { expected_samples: 2, completed_samples })) if completed_samples.is_empty())
    );
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Failed {
            failure: crate::RequestFailure::Stream
        }]
    );
    drop(observed(Box::pin(stream::pending()), Arc::clone(&observer)).await);
    assert_eq!(outcomes(&observer).len(), 1);
}

#[tokio::test]
async fn previews_unknown_extensions_and_partial_finals_cannot_establish_completion() {
    let unknown = ImageStreamChunk {
        event: crate::ImageStreamEvent::Unknown {
            event_type: "future-event".into(),
        },
        raw: crate::TransportSseEvent {
            data: "opaque extension".into(),
            ..Default::default()
        }
        .into(),
    };
    for chunks in [
        vec![preview(0), preview(1)],
        vec![unknown],
        vec![chunk(0), preview(1)],
    ] {
        let observer = Arc::new(Observer::default());
        let output = observed(
            Box::pin(stream::iter(chunks.into_iter().map(Ok))),
            Arc::clone(&observer),
        )
        .await
        .collect::<Vec<_>>()
        .await;
        assert!(matches!(
            output.last().unwrap(),
            Err(crate::BridgeError::Stream(
                crate::ImageStreamError::Incomplete {
                    expected_samples: 2,
                    ..
                }
            ))
        ));
        assert_eq!(
            outcomes(&observer),
            [RequestOutcome::Failed {
                failure: crate::RequestFailure::Stream
            }]
        );
    }
}

#[tokio::test]
async fn sample_indices_and_duplicate_finals_are_rejected() {
    for (chunks, duplicate) in [
        (vec![chunk(0), chunk(0), chunk(1)], true),
        (vec![chunk(0), chunk(2)], false),
        (vec![preview(2)], false),
    ] {
        let observer = Arc::new(Observer::default());
        let output = observed(
            Box::pin(stream::iter(chunks.into_iter().map(Ok))),
            Arc::clone(&observer),
        )
        .await
        .collect::<Vec<_>>()
        .await;
        match output.last().unwrap() {
            Err(crate::BridgeError::Stream(crate::ImageStreamError::DuplicateFinal {
                samp_ix: 0,
                ..
            })) if duplicate => {},
            Err(crate::BridgeError::Stream(crate::ImageStreamError::InvalidSample {
                samp_ix: 2,
                expected_samples: 2,
                ..
            })) if !duplicate => {},
            other => panic!("unexpected result {other:?}"),
        }
        assert_eq!(
            outcomes(&observer),
            [RequestOutcome::Failed {
                failure: crate::RequestFailure::Stream
            }]
        );
    }
}

#[tokio::test]
async fn first_final_does_not_end_multi_sample_stream_and_trailing_errors_still_fail() {
    let observer = Arc::new(Observer::default());
    let input = Box::pin(stream::iter([Ok(chunk(1)), Ok(preview(0)), Ok(chunk(0))]));
    let mut output = observed(input, Arc::clone(&observer)).await;
    assert_eq!(
        output.next().await.unwrap().unwrap().sample_index(),
        Some(1)
    );
    assert_eq!(outcomes(&observer), []);
    assert!(output.next().await.expect("expected final image").is_ok());
    assert!(output.next().await.expect("expected final image").is_ok());
    assert_eq!(outcomes(&observer), []);
    assert!(output.next().await.is_none());
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Succeeded { image_count: 2 }]
    );

    let observer = Arc::new(Observer::default());
    let error = DecodeError::new(DecodeTarget::StreamChunk, "trailing failure".into());
    let input = Box::pin(stream::iter([
        Ok(chunk(0)),
        Ok(chunk(1)),
        Err(error.into()),
    ]));
    let output = observed(input, Arc::clone(&observer))
        .await
        .collect::<Vec<_>>()
        .await;
    assert!(output.last().unwrap().is_err());
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Failed {
            failure: crate::RequestFailure::Decode
        }]
    );
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

#[derive(Default)]
struct DelayedObserver {
    released: AtomicBool,
    started: AtomicBool,
    outcomes: Mutex<Vec<RequestOutcome>>,
}

#[async_trait]
impl CreditObserver for DelayedObserver {
    async fn observe(&self, event: CreditEvent) {
        if let CreditEvent::RequestSettled { outcome, .. } = event {
            self.started.store(true, Ordering::Relaxed);
            futures_util::future::poll_fn(|_| {
                if self.released.load(Ordering::Relaxed) {
                    std::task::Poll::Ready(())
                } else {
                    std::task::Poll::Pending
                }
            })
            .await;
            self.outcomes
                .lock()
                .expect("observer mutex is not poisoned")
                .push(outcome);
        }
    }
}

async fn abandon_next_during_settlement(
    input: ImageStreamResult,
    consume_final: bool,
) -> (GenerateImageStream, Arc<DelayedObserver>) {
    let observer = Arc::new(DelayedObserver::default());
    let pending = ObservationContext::new(Arc::<DelayedObserver>::clone(&observer))
        .begin(
            1,
            ObservedOperation::EncodeVibe {
                model: Model::NaiDiffusion45Full,
            },
            RequestCostEstimate::Fixed { anlas: 2 },
        )
        .await;
    let mut output = GenerateImageStream::new(10, 1, input, pending);
    if consume_final {
        assert!(output.next().await.expect("expected final image").is_ok());
    }
    {
        let mut next = Box::pin(output.next());
        assert!(futures_util::poll!(next.as_mut()).is_pending());
    }
    assert!(observer.started.load(Ordering::Relaxed));
    assert_eq!(
        *observer
            .outcomes
            .lock()
            .expect("observer mutex is not poisoned"),
        []
    );
    observer.released.store(true, Ordering::Relaxed);
    (output, observer)
}

async fn cancel_during_settlement(input: ImageStreamResult, expected: RequestOutcome) {
    let (output, observer) =
        abandon_next_during_settlement(input, matches!(expected, RequestOutcome::Succeeded { .. }))
            .await;
    output.cancel().await;
    assert_eq!(
        *observer
            .outcomes
            .lock()
            .expect("observer mutex is not poisoned"),
        [expected]
    );
}

#[tokio::test]
async fn resumed_next_delivers_failure_once_after_settlement_was_pending() {
    let error = DecodeError::new(DecodeTarget::StreamChunk, "invalid stream".into());
    let (mut output, observer) =
        abandon_next_during_settlement(Box::pin(stream::iter([Err(error.into())])), false).await;
    assert!(output.next().await.unwrap().is_err());
    assert!(output.next().await.is_none());
    output.cancel().await;
    assert_eq!(
        *observer
            .outcomes
            .lock()
            .expect("observer mutex is not poisoned"),
        [RequestOutcome::Failed {
            failure: crate::RequestFailure::Decode,
        }]
    );
}

#[tokio::test]
async fn explicit_cancel_finishes_success_notification_after_next_is_abandoned() {
    cancel_during_settlement(
        Box::pin(stream::iter([Ok(chunk(0))])),
        RequestOutcome::Succeeded { image_count: 1 },
    )
    .await;
}

#[tokio::test]
async fn explicit_cancel_finishes_failure_notification_after_next_is_abandoned() {
    let error = DecodeError::new(DecodeTarget::StreamChunk, "invalid stream".into());
    cancel_during_settlement(
        Box::pin(stream::iter([Err(error.into())])),
        RequestOutcome::Failed {
            failure: crate::RequestFailure::Decode,
        },
    )
    .await;
}

#[tokio::test]
async fn eight_final_samples_settle_with_the_complete_small_canvas_batch() {
    let observer = Arc::new(Observer::default());
    let pending = ObservationContext::new(Arc::<Observer>::clone(&observer))
        .begin(
            1,
            ObservedOperation::EncodeVibe {
                model: Model::NaiDiffusion3,
            },
            RequestCostEstimate::Fixed { anlas: 2 },
        )
        .await;
    let input = Box::pin(stream::iter((0..8).map(|index| Ok(chunk(index)))));
    let output = GenerateImageStream::new(10, 8, input, pending);
    assert_eq!(output.collect::<Vec<_>>().await.len(), 8);
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Succeeded { image_count: 8 }]
    );
}

fn binary_final(sample: u32) -> serde_json::Value {
    serde_json::json!({"event_type":"final", "samp_ix":sample,
        "image":crate::generation::test_support::image().as_bytes()})
}

async fn binary_input(events: Vec<serde_json::Value>, truncated_tail: bool) -> ImageStreamResult {
    let mut body = Vec::new();
    for event in events {
        let payload = rmp_serde::to_vec_named(&event).unwrap();
        body.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
        body.extend_from_slice(&payload);
    }
    if truncated_tail {
        body.push(0);
    }
    crate::protocol::decode_stream_response(
        crate::TransportResponse::from_bytes(200, Vec::new(), body),
        "https://example.test/ai/generate-image-stream",
        crate::StreamMode::MessagePack,
        crate::ResponseLimits::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn messagepack_retains_the_shared_all_finals_and_eof_lifecycle() {
    let observer = Arc::new(Observer::default());
    let input = binary_input(vec![binary_final(1),
        serde_json::json!({"event_type":"intermediate", "samp_ix":0, "step_ix":1, "image":[1,2]}),
        binary_final(0), serde_json::json!({"event_type":"extension"})], false).await;
    let mut output = observed(input, Arc::clone(&observer)).await;
    for _ in 0..4 {
        assert!(output.next().await.unwrap().is_ok());
        assert_eq!(outcomes(&observer), []);
    }
    assert!(output.next().await.is_none());
    assert_eq!(
        outcomes(&observer),
        [RequestOutcome::Succeeded { image_count: 2 }]
    );

    for (events, truncated) in [
        (vec![binary_final(0)], false),
        (vec![binary_final(0), binary_final(0)], false),
        (vec![binary_final(2)], false),
        (
            vec![
                binary_final(0),
                binary_final(1),
                serde_json::json!({"event_type":"error", "message":"trailing"}),
            ],
            false,
        ),
        (vec![binary_final(0), binary_final(1)], true),
    ] {
        let observer = Arc::new(Observer::default());
        let input = binary_input(events, truncated).await;
        let output = observed(input, Arc::clone(&observer))
            .await
            .collect::<Vec<_>>()
            .await;
        assert!(output.last().unwrap().is_err());
        assert!(matches!(
            outcomes(&observer).as_slice(),
            [RequestOutcome::Failed { .. }]
        ));
    }
}
