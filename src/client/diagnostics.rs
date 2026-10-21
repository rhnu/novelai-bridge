use std::{error::Error, fmt};

use crate::BridgeError;

/// Records a failure once at the client boundary without server-echoed request data.
pub(super) fn report_failure(error: &BridgeError) {
    match error {
        #[cfg(feature = "streaming")]
        BridgeError::Stream(_) => {
            // Provider messages and extension data may echo prompts; callers retain the error.
            tracing::error!("image stream failed");
        },
        BridgeError::Api(api) => {
            if api.status() >= 500 {
                tracing::error!(status = api.status(), kind = ?api.kind(), body_truncated = api.raw_body_truncated, "API request failed");
            } else {
                tracing::warn!(status = api.status(), kind = ?api.kind(), body_truncated = api.raw_body_truncated, "API request rejected");
            }
        },
        BridgeError::InvalidRequest(_) => {
            tracing::debug!(causes = %ErrorChain(error), "local request rejected");
        },
        BridgeError::Transport(transport) => {
            tracing::error!(operation = %transport.operation, causes = %ErrorChain(error), "transport failed");
        },
        BridgeError::Decode(decode) => {
            tracing::error!(target = %decode.target, causes = %ErrorChain(error), "response decoding failed");
        },
        BridgeError::Encode(encode) => {
            tracing::error!(target = %encode.target, causes = %ErrorChain(error), "request encoding failed");
        },
        BridgeError::Preparation(_) => {
            tracing::error!(causes = %ErrorChain(error), "request preparation failed");
        },
        BridgeError::Metadata(_) => {
            tracing::error!(causes = %ErrorChain(error), "metadata processing failed");
        },
        #[cfg(feature = "tokenizer")]
        BridgeError::Tokenizer(_) => {
            tracing::error!(causes = %ErrorChain(error), "tokenizer failed");
        },
    }
}

struct ErrorChain<'a>(&'a (dyn Error + 'static));

impl fmt::Display for ErrorChain<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(formatter, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}
