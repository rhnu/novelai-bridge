use std::pin::Pin;

use futures_core::Stream;
pub use secrecy::SecretString;

mod client;
mod error;
mod internal;
mod metadata;
mod transport;
mod types;

pub use client::{Client, ClientOptions};
pub use error::{BridgeError, BridgeErrorCode};
pub use metadata::{
    PngMetadata, VibeMetadataEntry, VibeMetadataSource,
    extract_embedded_vibe_document_from_png_bytes, parse_png_metadata_from_bytes,
    parse_png_metadata_from_payload,
};
pub use transport::{ReqwestTransport, Transport};
pub use types::{
    ApiKeySource, Character, CharacterPosition, CharacterReference, CharacterReferenceType,
    ControlNetConfig, ControlNetInput, DirectorTool, EncodeVibeRequest, GenerateImageRequest,
    GenerateImageStreamRequest, GeneratedImage, ImageFormat, ImageSize, ImageStreamChunk,
    Img2ImgRequest, Model, NoiseSchedule, RunDirectorToolRequest, Sampler, StreamMode,
    SubscriptionInfo, UcPreset,
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
