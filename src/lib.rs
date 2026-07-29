use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
pub use secrecy::SecretString;

mod client;
mod error;
pub(crate) mod internal;
mod metadata;
mod transport;
mod types;

pub use client::{BaseUrls, Client, ClientOptions};
pub use error::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DataUrlError, DecodeError, DecodeTarget,
    InvalidRequest, MetadataError, TransportError, TransportOperation,
};
pub use metadata::{
    GeneratedImageMetadata, PngMetadata, PngMetadataWarning, VibeMetadataEntry, VibeMetadataSource,
    extract_embedded_vibe_document_from_png_bytes, inspect_generated_image_metadata,
    parse_png_metadata_from_bytes, parse_png_metadata_from_payload,
};
pub use transport::{ReqwestTransport, Transport};
pub use types::{
    ApiKeySource, Character, CharacterPosition, CharacterReference, CharacterReferenceType,
    ControlNetConfig, ControlNetInput, DirectorTool, EncodeVibeRequest, GenerateImageRequest,
    GenerateImageResult, GenerateImageStreamRequest, GeneratedImage, ImageFormat, ImageSize,
    ImageStreamChunk, Img2ImgRequest, Model, NoiseSchedule, RunDirectorToolRequest, Sampler,
    StreamMode, SubscriptionInfo, UcPreset,
};

/// Default base URL for image-generation endpoints.
pub const DEFAULT_IMAGE_BASE: &str = "https://image.novelai.net";
/// Default base URL for non-image API endpoints.
pub const DEFAULT_API_BASE: &str = "https://api.novelai.net";
/// Default HTTP timeout used by [`ClientOptions`], in milliseconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// A single parsed item yielded by [`Client::generate_stream`].
pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
/// The streaming result returned by [`Client::generate_stream`].
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

/// Streaming generation result with the request-level seed actually sent to `NovelAI`.
pub struct GenerateImageStream {
    /// Request-level seed actually sent after resolving an input seed of `0`.
    pub resolved_seed: i64,
    stream: ImageStreamResult,
}

impl GenerateImageStream {
    pub(crate) fn new(resolved_seed: i64, stream: ImageStreamResult) -> Self {
        Self {
            resolved_seed,
            stream,
        }
    }

    /// Consumes the wrapper and returns the underlying parsed chunk stream.
    #[must_use]
    pub fn into_inner(self) -> ImageStreamResult {
        self.stream
    }
}

impl Stream for GenerateImageStream {
    type Item = ImageStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(context)
    }
}
