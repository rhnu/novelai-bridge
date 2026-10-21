use std::pin::Pin;

use futures_core::Stream;
use futures_util::StreamExt;

use super::SseInputError;
use crate::TransportByteStream;

/// Feed complete, normalized lines so the SSE parser never rescans an unfinished
/// image line or copies a coalesced chunk's suffix after every parsed line.
pub(super) fn lines(
    mut chunks: TransportByteStream,
    limit: usize,
) -> Pin<Box<dyn Stream<Item = Result<Vec<u8>, SseInputError>> + Send>> {
    Box::pin(async_stream::try_stream! {
        let mut budget = crate::protocol::limits::SseFrameBudget::new(limit);
        let mut utf8 = Utf8Validator::default();
        let mut line = Vec::new();
        let mut first_line = true;
        let mut skip_lf = false;
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(SseInputError::Transport)?;
            budget.check(&chunk).map_err(SseInputError::Limit)?;
            utf8.check(&chunk).map_err(SseInputError::Utf8)?;
            let mut remaining = chunk.as_slice();
            while !remaining.is_empty() {
                if skip_lf {
                    skip_lf = false;
                    if remaining[0] == b'\n' {
                        remaining = &remaining[1..];
                        continue;
                    }
                }
                let Some(end) = remaining.iter().position(|byte| matches!(byte, b'\r' | b'\n')) else {
                    line.extend_from_slice(remaining);
                    break;
                };
                line.extend_from_slice(&remaining[..end]);
                skip_lf = remaining[end] == b'\r';
                remaining = &remaining[end + 1..];
                if first_line {
                    first_line = false;
                    // The dependency's BOM handling slices at a byte index inside the BOM.
                    if line.starts_with(b"\xEF\xBB\xBF") {
                        line.drain(..3);
                    }
                }
                line.push(b'\n');
                yield std::mem::take(&mut line);
            }
        }
        utf8.finish().map_err(SseInputError::Utf8)?;
        // SSE discards an unterminated line and never dispatches an event merely at EOF.
    })
}

/// Only an incomplete code point may survive a chunk; invalid sequences fail immediately.
#[derive(Default)]
struct Utf8Validator {
    pending: [u8; 4],
    len: usize,
}

impl Utf8Validator {
    fn check(&mut self, mut bytes: &[u8]) -> Result<(), std::str::Utf8Error> {
        while self.len != 0 && !bytes.is_empty() {
            let count = (self.pending.len() - self.len).min(bytes.len());
            self.pending[self.len..self.len + count].copy_from_slice(&bytes[..count]);
            bytes = &bytes[count..];
            self.len += count;
            match std::str::from_utf8(&self.pending[..self.len]) {
                Ok(_) => self.len = 0,
                Err(error) if error.error_len().is_some() => return Err(error),
                Err(error) => {
                    self.pending.copy_within(error.valid_up_to()..self.len, 0);
                    self.len -= error.valid_up_to();
                },
            }
        }
        if !bytes.is_empty() {
            match std::str::from_utf8(bytes) {
                Ok(_) => {},
                Err(error) if error.error_len().is_some() => return Err(error),
                Err(error) => {
                    let suffix = &bytes[error.valid_up_to()..];
                    self.pending[..suffix.len()].copy_from_slice(suffix);
                    self.len = suffix.len();
                },
            }
        }
        Ok(())
    }

    fn finish(&self) -> Result<(), std::str::Utf8Error> {
        std::str::from_utf8(&self.pending[..self.len]).map(|_| ())
    }
}
