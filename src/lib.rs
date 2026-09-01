#[cfg(feature = "streaming")]
use std::{
    collections::BTreeSet,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

#[cfg(feature = "streaming")]
use futures_core::Stream;
#[cfg(feature = "reqwest-transport")]
pub use secrecy::SecretString;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "generation")]
mod director;
mod error;
#[cfg(feature = "generation")]
mod generation;
#[cfg(feature = "image-processing")]
mod image;
#[cfg(feature = "png-metadata")]
mod metadata;
mod model;
#[cfg(feature = "client")]
mod observation;
mod parameters;
mod pricing;
#[cfg(feature = "client")]
mod protocol;
#[cfg(feature = "client")]
mod subscription;
#[cfg(feature = "tokenizer")]
mod tokenizer;
#[cfg(feature = "client")]
mod transport;
#[cfg(feature = "generation")]
mod vibe;

#[cfg(feature = "reqwest-transport")]
pub use client::{ApiKeySource, ClientOptions};
#[cfg(feature = "client")]
pub use client::{BaseUrls, Client};
#[cfg(feature = "generation")]
pub use director::{DirectorCommand, RunDirectorToolRequest};
pub use error::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DataUrlError, DecodeError, DecodeTarget,
    InvalidRequest, MetadataError, TransportError, TransportOperation,
};
#[cfg(feature = "generation")]
pub use generation::{
    Character, CharacterPosition, CharacterReference, CharacterReferenceType, CharacterSet,
    GenerateImageRequest, GenerateImageRequestBuilder, GenerateImageResult,
    GenerateImageStreamRequest, GeneratedImage, GenerationCore, GuidanceParams, ImageStreamChunk,
    Img2ImgRequest, OutputParams, ReferenceSet, ResolvedCharacterPrompt, ResolvedPrompts,
};
#[cfg(feature = "generation")]
pub use generation::{
    NormalizationChange, NormalizationDiagnostic, NormalizationPolicy, NormalizedGeneration,
    PreparedGeneration,
};
#[cfg(feature = "image-processing")]
pub use image::{
    DefaultImagePreprocessor, EncodedImage, ImagePreparation, ImagePreprocessor, PreparedImage,
    VibeEncoding, decode_generated_images,
};
#[cfg(feature = "png-metadata")]
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
#[cfg(all(feature = "client", not(feature = "credit-observer")))]
pub(crate) use observation::{
    CreditBalance, CreditEvent, CreditObserver, ObservedOperation, RequestCostEstimate,
    RequestFailure, RequestFingerprint, RequestOutcome,
};
#[cfg(feature = "credit-observer")]
pub use observation::{
    CreditBalance, CreditEvent, CreditObserver, ObservedOperation, RequestCostEstimate,
    RequestFailure, RequestFingerprint, RequestOutcome,
};
pub use parameters::{
    ImageFormat, ImageSize, NoiseSchedule, QualityPreset, Sampler, SamplingParams, StreamMode,
    UcPreset,
};
pub use pricing::{
    AnlasEstimate, AnlasEstimateInput, AnlasEstimateStatus, AnlasPricingContext,
    estimate_anlas_cost, estimate_director_tool_cost,
};
#[cfg(feature = "client")]
pub use subscription::{SubscriptionInfo, V5UsageStatus};
#[cfg(feature = "tokenizer")]
pub use tokenizer::{
    CharacterTokenUsage, ClipTokenizer, FramingPolicy, ImageTokenizer, PromptField,
    PromptTokenUsage, QwenTokenizer, T5Tokenizer, TokenCount, TokenId, TokenizerAsset,
    TokenizerError, Tokenizers,
};
#[cfg(feature = "reqwest-transport")]
pub use transport::ReqwestTransport;
#[cfg(feature = "streaming")]
pub use transport::{
    StreamingTransport, TransportSseBody, TransportSseEvent, TransportSseResponse,
    TransportSseStream,
};
#[cfg(feature = "client")]
pub use transport::{
    Transport, TransportHeader, TransportMethod, TransportRequest, TransportResponse,
};
#[cfg(feature = "generation")]
pub use vibe::{EncodeVibeRequest, VibeReference, VibeTransferConfig};

/// Default base URL for image-generation endpoints.
#[cfg(feature = "client")]
pub const DEFAULT_IMAGE_BASE: &str = "https://image.novelai.net";
/// Default base URL for non-image API endpoints.
#[cfg(feature = "client")]
pub const DEFAULT_API_BASE: &str = "https://api.novelai.net";
/// Default HTTP timeout used by [`ClientOptions`], in milliseconds.
#[cfg(feature = "reqwest-transport")]
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// A single parsed item yielded by [`Client::generate_stream`].
#[cfg(feature = "streaming")]
pub type ImageStreamItem = Result<ImageStreamChunk, BridgeError>;
/// The streaming result returned by [`Client::generate_stream`].
#[cfg(feature = "streaming")]
pub type ImageStreamResult = Pin<Box<dyn Stream<Item = ImageStreamItem> + Send>>;

/// Streaming generation result with the request-level seed actually sent to `NovelAI`.
#[cfg(feature = "streaming")]
pub struct GenerateImageStream {
    /// Request-level seed actually sent after resolving an input seed of `0`.
    pub resolved_seed: i64,
    stream: ImageStreamResult,
    observation: Arc<Mutex<Option<crate::client::StreamObservation>>>,
}

#[cfg(feature = "streaming")]
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

#[cfg(feature = "streaming")]
impl Stream for GenerateImageStream {
    type Item = ImageStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.as_mut().poll_next(context)
    }
}

#[cfg(feature = "streaming")]
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
