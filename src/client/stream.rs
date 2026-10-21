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
use crate::{ImageStreamError, ImageStreamEvent};

type SettlementFuture = Pin<Box<dyn Future<Output = ()> + Send>>;

/// The handle retains terminal delivery when a polling future is cancelled.
enum Lifecycle {
    Active {
        stream: ImageStreamResult,
        pending: PendingObservation,
    },
    Settling {
        future: SettlementFuture,
        terminal: Option<ImageStreamItem>,
    },
    Settled,
}

/// Parsed image-generation stream and the request seed actually sent.
pub struct GenerateImageStream {
    pub resolved_seed: i64,
    lifecycle: Lifecycle,
    samples: BTreeSet<u32>,
    expected_samples: u32,
    span: tracing::Span,
    started: Instant,
}

impl GenerateImageStream {
    pub(super) fn new(
        resolved_seed: i64,
        expected_samples: u32,
        stream: ImageStreamResult,
        observation: PendingObservation,
    ) -> Self {
        Self {
            resolved_seed,
            lifecycle: Lifecycle::Active {
                stream,
                pending: observation,
            },
            samples: BTreeSet::new(),
            expected_samples,
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
        let lifecycle = std::mem::replace(&mut self.lifecycle, Lifecycle::Settled);
        let span = self.span.clone();
        let elapsed_secs = self.started.elapsed().as_secs_f64();
        async move {
            match lifecycle {
                Lifecycle::Active { stream, pending } => {
                    drop(stream);
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

    fn begin_settlement(
        &mut self,
        pending: PendingObservation,
        mut terminal: Option<ImageStreamItem>,
    ) {
        if terminal.is_none() && self.samples.len() != self.expected_samples as usize {
            terminal = Some(Err(ImageStreamError::Incomplete {
                expected_samples: self.expected_samples,
                completed_samples: self.samples.iter().copied().collect(),
            }
            .into()));
        }
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
            match std::mem::replace(&mut self.lifecycle, Lifecycle::Settled) {
                Lifecycle::Active {
                    mut stream,
                    pending,
                } => match stream.as_mut().poll_next(context) {
                    Poll::Ready(Some(Ok(chunk))) => {
                        let failure = if matches!(chunk.event, ImageStreamEvent::Error { .. }) {
                            Some(ImageStreamError::Provider {
                                event: Box::new(chunk.clone()),
                            })
                        } else if let Some(samp_ix) = chunk.sample_index() {
                            if samp_ix >= self.expected_samples {
                                Some(ImageStreamError::InvalidSample {
                                    samp_ix,
                                    expected_samples: self.expected_samples,
                                    raw: Box::new(chunk.raw.clone()),
                                })
                            } else if matches!(chunk.event, ImageStreamEvent::Final { .. })
                                && !self.samples.insert(samp_ix)
                            {
                                Some(ImageStreamError::DuplicateFinal {
                                    samp_ix,
                                    raw: Box::new(chunk.raw.clone()),
                                })
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        if let Some(error) = failure {
                            drop(stream);
                            self.begin_settlement(pending, Some(Err(error.into())));
                            continue;
                        }
                        self.lifecycle = Lifecycle::Active { stream, pending };
                        return Poll::Ready(Some(Ok(chunk)));
                    },
                    Poll::Ready(terminal) => {
                        drop(stream);
                        self.begin_settlement(pending, terminal);
                    },
                    Poll::Pending => {
                        self.lifecycle = Lifecycle::Active { stream, pending };
                        return Poll::Pending;
                    },
                },
                Lifecycle::Settling {
                    mut future,
                    terminal,
                } => {
                    if future.as_mut().poll(context).is_pending() {
                        self.lifecycle = Lifecycle::Settling { future, terminal };
                        return Poll::Pending;
                    }
                    return Poll::Ready(terminal);
                },
                Lifecycle::Settled => return Poll::Ready(None),
            }
        }
    }
}

#[cfg(all(test, feature = "credit-observer"))]
mod tests;
