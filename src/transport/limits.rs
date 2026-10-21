/// Maximum bytes accepted at the HTTP and SSE framing boundaries.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ResponseLimits {
    /// Complete successful HTTP body, before image or archive decoding.
    pub success_body_bytes: usize,
    /// Retained prefix of a non-success HTTP body.
    pub error_body_bytes: usize,
    /// Raw bytes in one SSE frame, including its line delimiters.
    pub sse_event_bytes: usize,
}

impl Default for ResponseLimits {
    fn default() -> Self {
        Self {
            success_body_bytes: 256 * 1024 * 1024,
            error_body_bytes: 64 * 1024,
            sse_event_bytes: 64 * 1024 * 1024,
        }
    }
}

/// A response exceeded its byte budget before it could be buffered or framed.
#[derive(Debug, thiserror::Error)]
#[error("response exceeds the {limit}-byte limit")]
pub struct ResponseLimitExceeded {
    pub limit: usize,
}

/// Retains a bounded HTTP body, rejecting success overflow and truncating error bodies.
#[cfg(any(feature = "reqwest-transport", test))]
pub(super) struct BodyBuffer {
    bytes: Vec<u8>,
    limit: usize,
    truncate: bool,
}

#[cfg(any(feature = "reqwest-transport", test))]
impl BodyBuffer {
    pub(super) const fn new(limit: usize, truncate: bool) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            truncate,
        }
    }

    /// Returns false when the body has been truncated and reading should stop.
    pub(super) fn append(&mut self, chunk: &[u8]) -> Result<bool, ResponseLimitExceeded> {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if chunk.len() > remaining {
            if !self.truncate {
                return Err(ResponseLimitExceeded { limit: self.limit });
            }
            self.bytes.extend_from_slice(&chunk[..remaining]);
            return Ok(false);
        }
        self.bytes.extend_from_slice(chunk);
        Ok(true)
    }

    pub(super) fn into_bytes(self) -> Vec<u8> { self.bytes }
}

/// Scans framing delimiters before passing chunks to the SSE parser.
///
/// A CR is resolved on the following byte so split CRLF sequences count as one delimiter
/// without accidentally charging a following frame for the previous frame's LF.
#[cfg(feature = "reqwest-streaming")]
pub(super) struct SseFrameBudget {
    limit: usize,
    bytes: usize,
    line_has_data: bool,
    pending_cr: bool,
}

#[cfg(feature = "reqwest-streaming")]
impl SseFrameBudget {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            limit,
            bytes: 0,
            line_has_data: false,
            pending_cr: false,
        }
    }

    pub(super) fn check(&mut self, chunk: &[u8]) -> Result<(), ResponseLimitExceeded> {
        for &byte in chunk {
            if self.pending_cr {
                self.pending_cr = false;
                if byte == b'\n' {
                    self.count_byte()?;
                    self.end_line();
                    continue;
                }
                self.end_line();
            }
            self.count_byte()?;
            match byte {
                b'\r' => self.pending_cr = true,
                b'\n' => self.end_line(),
                _ => self.line_has_data = true,
            }
        }
        Ok(())
    }

    const fn count_byte(&mut self) -> Result<(), ResponseLimitExceeded> {
        if self.bytes >= self.limit {
            return Err(ResponseLimitExceeded { limit: self.limit });
        }
        self.bytes += 1;
        Ok(())
    }

    const fn end_line(&mut self) {
        if !self.line_has_data {
            self.bytes = 0;
        }
        self.line_has_data = false;
    }
}

#[cfg(test)]
mod tests;
