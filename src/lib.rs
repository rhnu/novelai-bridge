use std::{
    collections::BTreeSet,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use futures_core::Stream;
pub use secrecy::SecretString;

mod client;
mod credit;
mod error;
mod generation;
mod image;
pub(crate) mod internal;
mod metadata;
mod model;
mod protocol;
#[cfg(feature = "tokenizer")]
mod tokenizer;
mod transport;
mod types;

pub use client::{BaseUrls, Client, ClientOptions};
pub use credit::{
    AnlasEstimate, AnlasEstimateInput, AnlasEstimateStatus, AnlasPricingContext, CreditBalance,
    CreditEvent, CreditObserver, ObservedOperation, RequestCostEstimate, RequestFailure,
    RequestFingerprint, RequestOutcome, estimate_anlas_cost, estimate_director_tool_cost,
};
pub use error::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DataUrlError, DecodeError, DecodeTarget,
    InvalidRequest, MetadataError, TransportError, TransportOperation,
};
pub use generation::{
    NormalizationChange, NormalizationDiagnostic, NormalizationPolicy, NormalizedGeneration,
    PreparedGeneration,
};
pub use image::{
    DefaultImagePreprocessor, EncodedImage, ImagePreparation, ImagePreprocessor, PreparedImage,
    VibeEncoding, decode_generated_images,
};
pub use metadata::{
    GeneratedImageMetadata, PngMetadata, PngMetadataWarning, VibeMetadataEntry, VibeMetadataSource,
    extract_embedded_vibe_document_from_png_bytes, inspect_generated_image_metadata,
    parse_png_metadata_from_bytes, parse_png_metadata_from_payload,
};
pub use model::{
    CanvasConstraints, CharacterPromptProfile, GenerationConstraints, GenerationMode,
    ImageTokenizerKind, InpaintingProfile, Model, ModelDefaults, ModelDescriptor, ModelIdentity,
    ModelSupport, NoiseScheduleSupport, OpusAllowance, ParameterRange, PricingProfile,
    PromptProfile, PromptStructure, QualitySupport, ReferenceAvailability, ReferenceIntent,
    ReferenceModes, ReferenceProfile, ReferenceResolution, SamplingProfile, VarietyBoostProfile,
    VibeInputPath, VibeProfile, WireProfile,
};
#[cfg(feature = "tokenizer")]
pub use tokenizer::{
    CharacterTokenUsage, ClipTokenizer, FramingPolicy, ImageTokenizer, PromptField,
    PromptTokenUsage, QwenTokenizer, T5Tokenizer, TokenCount, TokenId, TokenizerAsset,
    TokenizerError, Tokenizers,
};
pub use transport::{
    ReqwestTransport, StreamingTransport, Transport, TransportHeader, TransportMethod,
    TransportRequest, TransportResponse, TransportSseBody, TransportSseEvent, TransportSseResponse,
    TransportSseStream,
};
pub use types::{
    ApiKeySource, Character, CharacterPosition, CharacterReference, CharacterReferenceType,
    CharacterSet, DirectorTool, EncodeVibeRequest, GenerateImageRequest,
    GenerateImageRequestBuilder, GenerateImageResult, GenerateImageStreamRequest, GeneratedImage,
    GenerationCore, GuidanceParams, ImageFormat, ImageSize, ImageStreamChunk, Img2ImgRequest,
    NoiseSchedule, OutputParams, QualityPreset, ReferenceSet, ResolvedCharacterPrompt,
    ResolvedPrompts, RunDirectorToolRequest, Sampler, SamplingParams, StreamMode, SubscriptionInfo,
    UcPreset, V5UsageStatus, VibeReference, VibeTransferConfig,
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
    observation: Arc<Mutex<Option<crate::client::StreamObservation>>>,
}

impl GenerateImageStream {
    pub(crate) fn new(
        resolved_seed: i64,
        stream: ImageStreamResult,
        observation: Option<crate::client::StreamObservation>,
    ) -> Self {
        let observation = Arc::new(Mutex::new(observation));
        let stream = observed_stream(stream, observation.clone());
        Self {
            resolved_seed,
            stream,
            observation,
        }
    }

    /// Consumes the wrapper and returns the underlying parsed chunk stream.
    #[must_use]
    pub fn into_inner(self) -> ImageStreamResult {
        self.stream
    }

    /// Explicitly cancels stream consumption and emits one cancelled observation.
    ///
    /// Dropping a stream does not run asynchronous observer code and therefore emits no guessed
    /// terminal state. Call this method when cancellation must be recorded.
    pub async fn cancel(self) {
        let observation = self
            .observation
            .lock()
            .ok()
            .and_then(|mut value| value.take());
        if let Some(observation) = observation {
            observation.settle(RequestOutcome::Cancelled).await;
        }
    }
}

impl Stream for GenerateImageStream {
    type Item = ImageStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(context)
    }
}

fn observed_stream(
    mut input: ImageStreamResult,
    observation: Arc<Mutex<Option<crate::client::StreamObservation>>>,
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
                    let pending = observation.lock().ok().and_then(|mut value| value.take());
                    if let Some(pending) = pending {
                        pending.settle(RequestOutcome::Failed {
                            failure: RequestFailure::from_error(&error),
                        }).await;
                    }
                    yield Err(error);
                    return;
                }
            }
        }
        let pending = observation.lock().ok().and_then(|mut value| value.take());
        if let Some(pending) = pending {
            pending.settle(RequestOutcome::Succeeded {
                image_count: u32::try_from(samples.len()).unwrap_or(u32::MAX),
            }).await;
        }
    })
}
