use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;

use crate::{BridgeError, DecodeError, DecodeTarget, TransportByteStream};

pub(super) type MessagePackStream =
    Pin<Box<dyn Stream<Item = Result<Vec<u8>, BridgeError>> + Send>>;

fn framing_error(source: impl std::error::Error + Send + Sync + 'static) -> BridgeError {
    DecodeError::new(DecodeTarget::StreamChunk, Box::new(source)).into()
}

/// Reads a four-byte big-endian payload length before buffering any frame data.
/// Only one bounded payload is assembled; a coalesced transport chunk is consumed in place.
pub(super) fn decode(mut chunks: TransportByteStream, limit: usize) -> MessagePackStream {
    Box::pin(async_stream::try_stream! {
        let mut prefix = [0_u8; 4];
        let mut prefix_len = 0;
        let mut length = None;
        let mut payload = Vec::new();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk?;
            let mut remaining = chunk.as_slice();
            while !remaining.is_empty() {
                if length.is_none() {
                    let count = (4 - prefix_len).min(remaining.len());
                    prefix[prefix_len..prefix_len + count].copy_from_slice(&remaining[..count]);
                    prefix_len += count;
                    remaining = &remaining[count..];
                    if prefix_len != 4 { continue; }
                    let advertised = u64::from(u32::from_be_bytes(prefix));
                    if advertised > limit as u64 {
                        Err(framing_error(super::limits::ResponseLimitExceeded { limit }))?;
                    }
                    let advertised = usize::try_from(advertised).map_err(framing_error)?;
                    if advertised == 0 {
                        Err(framing_error(std::io::Error::new(std::io::ErrorKind::InvalidData,
                            "MessagePack frame has an empty payload")))?;
                    }
                    length = Some(advertised);
                }
                let expected = length.expect("complete prefix establishes a payload length");
                let count = (expected - payload.len()).min(remaining.len());
                payload.extend_from_slice(&remaining[..count]);
                remaining = &remaining[count..];
                if payload.len() == expected {
                    yield std::mem::take(&mut payload);
                    length = None;
                    prefix_len = 0;
                }
            }
        }
        if prefix_len != 0 {
            Err(framing_error(std::io::Error::new(std::io::ErrorKind::UnexpectedEof,
                "stream ended inside a MessagePack frame")))?;
        }
    })
}

#[cfg(test)]
mod tests;
