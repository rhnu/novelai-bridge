use std::{pin::Pin, task::{Context, Poll}};
#[cfg(feature = "credit-observer")]
use std::{collections::BTreeSet, sync::{Arc, Mutex}};

use futures_core::Stream;

use crate::{BridgeError, ImageStreamChunk};
use super::observe::PendingObservation;

pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

/// Parsed image-generation stream and the request seed actually sent.
pub struct GenerateImageStream {
    pub resolved_seed: i64,
    stream: ImageStreamResult,
    #[cfg(feature = "credit-observer")]
    observation: Arc<Mutex<Option<PendingObservation>>>,
}

impl GenerateImageStream {
    pub(super) fn new(resolved_seed: i64, stream: ImageStreamResult, observation: PendingObservation) -> Self {
        #[cfg(feature = "credit-observer")]
        {
            let observation = Arc::new(Mutex::new(Some(observation)));
            let stream = observed_stream(stream, observation.clone());
            Self { resolved_seed, stream, observation }
        }
        #[cfg(not(feature = "credit-observer"))]
        {
            let _ = observation;
            Self { resolved_seed, stream }
        }
    }

    /// Returns the parsed stream, retaining its lifecycle observation.
    #[must_use]
    pub fn into_inner(self) -> ImageStreamResult {
        self.stream
    }

    /// Stops consumption before awaiting the explicit cancellation notification.
    ///
    /// Dropping a stream emits no asynchronous terminal notification.
    pub async fn cancel(self) {
        #[cfg(feature = "credit-observer")]
        let pending = self.observation.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        drop(self.stream);
        #[cfg(feature = "credit-observer")]
        if let Some(pending) = pending { pending.cancel().await; }
    }
}

impl Stream for GenerateImageStream {
    type Item = ImageStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(context)
    }
}

#[cfg(feature = "credit-observer")]
fn observed_stream(
    mut input: ImageStreamResult,
    observation: Arc<Mutex<Option<PendingObservation>>>,
) -> ImageStreamResult {
    Box::pin(async_stream::stream! {
        let mut samples = BTreeSet::new();
        while let Some(item) = futures_util::StreamExt::next(&mut input).await {
            match item {
                Ok(chunk) => {
                    if !chunk.image.is_empty() { samples.insert(chunk.samp_ix); }
                    yield Ok(chunk);
                }
                Err(error) => {
                    let pending = observation.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
                    if let Some(pending) = pending { pending.fail(&error).await; }
                    yield Err(error);
                    return;
                }
            }
        }
        let pending = observation.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(pending) = pending {
            pending.succeed(u32::try_from(samples.len()).unwrap_or(u32::MAX)).await;
        }
    })
}

#[cfg(all(test, feature = "credit-observer"))]
mod tests;
