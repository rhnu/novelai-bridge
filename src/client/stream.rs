use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use futures_core::Stream;
use tracing::Instrument;

use super::observe::PendingObservation;
pub use crate::protocol::{ImageStreamItem, ImageStreamResult};

type SettlementFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The handle retains terminal delivery when a polling future is cancelled.
enum Lifecycle {
    Active(PendingObservation),
    Settling {
        future: SettlementFuture,
        terminal: Option<ImageStreamItem>,
    },
    Settled,
}

/// Parsed image-generation stream and the request seed actually sent.
pub struct GenerateImageStream {
    pub resolved_seed: i64,
    stream: Option<ImageStreamResult>,
    lifecycle: Lifecycle,
    samples: BTreeSet<u32>,
    span: tracing::Span,
    started: Instant,
}

impl GenerateImageStream {
    pub(super) fn new(
        resolved_seed: i64,
        stream: ImageStreamResult,
        observation: PendingObservation,
    ) -> Self {
        Self {
            resolved_seed,
            stream: Some(stream),
            lifecycle: Lifecycle::Active(observation),
            samples: BTreeSet::new(),
            span: tracing::Span::current(),
            started: Instant::now(),
        }
    }

    /// Returns the parsed stream, retaining its lifecycle observation.
    pub fn into_inner(self) -> ImageStreamResult { Box::pin(self) }

    /// Stops consumption and completes an already started terminal notification, if any.
    ///
    /// An active stream reports cancellation. An EOF or failure already being settled retains
    /// that outcome. Dropping a stream emits no asynchronous terminal notification.
    pub async fn cancel(mut self) {
        drop(self.stream.take());
        let lifecycle = std::mem::replace(&mut self.lifecycle, Lifecycle::Settled);
        let span = self.span.clone();
        let elapsed_secs = self.started.elapsed().as_secs_f64();
        async move {
            match lifecycle {
                Lifecycle::Active(pending) => {
                    tracing::debug!(outcome = "cancelled", elapsed_secs, "image stream stopped");
                    pending.cancel().await;
                },
                Lifecycle::Settling { future, .. } => future.await,
                Lifecycle::Settled => {},
            }
        }
        .instrument(span)
        .await;
    }

    fn begin_settlement(&mut self, terminal: Option<ImageStreamItem>) {
        drop(self.stream.take());
        let Lifecycle::Active(pending) = std::mem::replace(&mut self.lifecycle, Lifecycle::Settled)
        else {
            return;
        };
        let future: SettlementFuture = if let Some(Err(error)) = &terminal {
            tracing::debug!(
                outcome = "failed",
                elapsed_secs = self.started.elapsed().as_secs_f64(),
                "image stream stopped"
            );
            super::diagnostics::report_failure(error);
            Box::pin(pending.fail(error))
        } else {
            let image_count = u32::try_from(self.samples.len()).unwrap_or(u32::MAX);
            tracing::debug!(
                outcome = "succeeded",
                image_count,
                elapsed_secs = self.started.elapsed().as_secs_f64(),
                "image stream exhausted"
            );
            Box::pin(pending.succeed(image_count))
        };
        self.lifecycle = Lifecycle::Settling { future, terminal };
    }
}

impl Stream for GenerateImageStream {
    type Item = ImageStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // This guard is confined to a synchronous poll and never crosses an await boundary.
        let span = self.span.clone();
        let _entered = span.enter();
        loop {
            match &mut self.lifecycle {
                Lifecycle::Active(_) => {
                    let Some(input) = &mut self.stream else {
                        return Poll::Ready(None);
                    };
                    match input.as_mut().poll_next(context) {
                        Poll::Ready(Some(Ok(chunk))) => {
                            if !chunk.image.is_empty() {
                                self.samples.insert(chunk.samp_ix);
                            }
                            return Poll::Ready(Some(Ok(chunk)));
                        },
                        Poll::Ready(terminal) => self.begin_settlement(terminal),
                        Poll::Pending => return Poll::Pending,
                    }
                },
                Lifecycle::Settling { future, terminal } => {
                    if future.as_mut().poll(context).is_pending() {
                        return Poll::Pending;
                    }
                    let terminal = terminal.take();
                    self.lifecycle = Lifecycle::Settled;
                    return Poll::Ready(terminal);
                },
                Lifecycle::Settled => return Poll::Ready(None),
            }
        }
    }
}

#[cfg(all(test, feature = "credit-observer"))]
mod tests;
