use std::pin::Pin;

use eventsource_stream::{EventStreamError, Eventsource};
use futures_core::Stream;
use futures_util::StreamExt;

use crate::{
    BridgeError, DecodeError, DecodeTarget, TransportHeader, TransportSseEvent,
    transport::TransportByteStream,
};

mod input;

pub(super) type SseStream =
    Pin<Box<dyn Stream<Item = Result<TransportSseEvent, BridgeError>> + Send>>;

pub(super) fn is_event_stream(headers: &[TransportHeader]) -> bool {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .and_then(|header| std::str::from_utf8(&header.value).ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("text/event-stream"))
}

pub(super) fn decode(chunks: TransportByteStream, limit: usize) -> SseStream {
    let mut events = input::lines(chunks, limit).eventsource();
    Box::pin(async_stream::try_stream! {
        while let Some(event) = events.next().await {
            let event = event.map_err(|source| match source {
                EventStreamError::Transport(SseInputError::Transport(source)) => BridgeError::Transport(source),
                EventStreamError::Transport(SseInputError::Limit(source)) =>
                    DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)).into(),
                EventStreamError::Transport(SseInputError::Utf8(source)) =>
                    DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)).into(),
                source => DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)).into(),
            })?;
            yield TransportSseEvent {
                event: event.event, id: event.id, data: event.data, retry: event.retry,
            };
        }
    })
}

#[derive(Debug, thiserror::Error)]
enum SseInputError {
    #[error(transparent)]
    Transport(crate::TransportError),
    #[error(transparent)]
    Limit(super::limits::ResponseLimitExceeded),
    #[error(transparent)]
    Utf8(std::str::Utf8Error),
}

#[cfg(test)]
mod tests;
