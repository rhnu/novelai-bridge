use std::error::Error as StdError;

use thiserror::Error;

use crate::{ImageStreamChunk, TransportSseEvent};

/// Business or protocol failure of an opened image-generation stream.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ImageStreamError {
    /// The provider's error event, including its typed message and complete SSE source.
    #[error("provider reported an image-generation error")]
    Provider { event: Box<ImageStreamChunk> },
    #[error("invalid image-generation event")]
    InvalidEvent {
        raw: Box<TransportSseEvent>,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },
    #[error("invalid final image for sample {samp_ix}")]
    InvalidFinal {
        samp_ix: u32,
        raw: Box<TransportSseEvent>,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },
    #[error(
        "stream ended with missing final images: expected {expected_samples}, received {completed_samples:?}"
    )]
    Incomplete {
        expected_samples: u32,
        completed_samples: Vec<u32>,
    },
    #[error("stream sample {samp_ix} is outside the expected range 0..{expected_samples}")]
    InvalidSample {
        samp_ix: u32,
        expected_samples: u32,
        raw: Box<TransportSseEvent>,
    },
    #[error("stream repeated the final image for sample {samp_ix}")]
    DuplicateFinal {
        samp_ix: u32,
        raw: Box<TransportSseEvent>,
    },
}
