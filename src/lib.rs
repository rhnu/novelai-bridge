#[cfg(feature = "client")]
mod client;
#[cfg(feature = "generation")]
mod director;
mod error;
mod field;
#[cfg(feature = "generation")]
mod generation;
#[cfg(feature = "image-processing")]
mod image;
#[cfg(feature = "png-metadata")]
mod media_policy;
#[cfg(feature = "png-metadata")]
mod metadata;
mod model;
#[cfg(feature = "credit-observer")]
mod observation;
mod parameters;
#[cfg(feature = "png-metadata")]
mod payload;
mod pricing;
#[cfg(feature = "client")]
mod protocol;
#[cfg(feature = "client")]
mod subscription;
#[cfg(feature = "tokenizer")]
mod tokenizer;
#[cfg(feature = "client")]
mod transport;
mod validation;
#[cfg(feature = "png-metadata")]
mod vibe;

#[cfg(feature = "client")]
pub use client::{Client, ClientConfig, DEFAULT_IMAGE_BASE};
#[cfg(feature = "streaming")]
pub use client::{GenerateImageStream, ImageStreamItem, ImageStreamResult};
#[cfg(feature = "generation")]
pub use director::{DirectorCommand, PreparedDirector, RunDirectorToolRequest};
#[cfg(feature = "streaming")]
pub use error::ImageStreamError;
pub use error::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DataUrlError, DecodeError, DecodeTarget,
    EncodeError, EncodeTarget, InvalidRequest, MetadataError, PreparationError, TransportError,
    TransportOperation,
};
pub use field::{
    CharacterField, CharacterReferenceField, GenerationField, GuidanceField, ImageInputField,
    RequestField,
};
#[cfg(feature = "generation")]
pub use generation::{
    Character, CharacterPosition, CharacterReference, CharacterReferenceType, CharacterSet,
    GenerateImageRequest, GenerateImageRequestBuilder, GenerateImageResult,
    GenerateImageStreamRequest, GenerationCore, GenerationDelivery, GenerationInput,
    GuidanceParams, ImageStreamChunk, ImageStreamEvent, ImageStreamEventType, ImageToImageInput,
    InpaintingInput, NormalizationChange, NormalizationDiagnostic, NormalizedGeneration,
    OutputParams, PreparedGeneration, ReferenceSet, ResolvedCharacterPrompt, ResolvedPrompts,
    TransportSseEvent,
};
#[cfg(all(feature = "generation", feature = "tokenizer"))]
pub use generation::{CharacterTokenUsage, PromptField, PromptTokenUsage};
#[cfg(feature = "image-processing")]
pub use image::{
    DefaultImagePreprocessor, EncodedImage, GeneratedImage, ImageMediaFormat, ImagePreparation,
    ImagePreprocessor, PreparedImage, decode_generated_images, decode_generated_images_with_policy,
};
#[cfg(feature = "png-metadata")]
pub use media_policy::{ImageDecodePolicy, ImageLimit, ImagePolicyError};
#[cfg(feature = "png-metadata")]
pub use metadata::{
    CharacterMetadata, GeneratedImageMetadata, MetadataField, PngMetadata, PngMetadataWarning,
    VibeMetadataEntry, VibeMetadataSource, extract_embedded_vibe_document_from_png_bytes,
    inspect_generated_image_metadata, parse_png_metadata_from_bytes,
    parse_png_metadata_from_bytes_with_policy, parse_png_metadata_from_payload,
};
pub use model::{
    CanvasConstraints, CharacterPositionProfile, CharacterPromptProfile, GenerationConstraints,
    GenerationMode, ImageTokenizerKind, InpaintingProfile, Model, ModelDefaults, ModelDescriptor,
    ModelIdentity, ModelSupport, NoiseScheduleSupport, OpusAllowance, ParameterRange,
    PricingProfile, PromptProfile, PromptStructure, QualitySupport, ReferenceAvailability,
    ReferenceIntent, ReferenceModes, ReferenceProfile, ReferenceResolution, SamplingProfile,
    VarietyBoostProfile, VibeInputPath, VibeProfile, WireProfile,
};
#[cfg(feature = "credit-observer")]
pub use observation::{
    CreditEvent, CreditObserver, GenerationSummary, ObservedDirectorTool, ObservedOperation,
    RequestCostEstimate, RequestFailure, RequestFingerprint, RequestOutcome,
};
pub use parameters::{
    ImageFormat, ImageSize, NoiseSchedule, QualityPreset, Sampler, SamplingParams, StreamMode,
    UcPreset,
};
pub use pricing::{
    AnlasEstimate, AnlasEstimateInput, AnlasEstimateStatus, AnlasPricingContext, PricingMode,
    estimate_anlas_cost, estimate_director_tool_cost,
};
#[cfg(feature = "client")]
pub use protocol::ApiEndpoint;
#[cfg(feature = "reqwest-transport")]
pub use secrecy::SecretString;
#[cfg(feature = "client")]
pub use subscription::{SubscriptionInfo, V5UsageStatus};
#[cfg(feature = "tokenizer")]
pub use tokenizer::{
    ClipTokenizer, FramingPolicy, QwenTokenizer, T5Tokenizer, TokenCount, TokenEncoder, TokenId,
    TokenizerAsset, TokenizerError, Tokenizers,
};
#[cfg(feature = "reqwest-transport")]
pub use transport::{ApiKeySource, DEFAULT_TIMEOUT_MS, ReqwestOptions, ReqwestTransport};
#[cfg(feature = "client")]
pub use transport::{
    ResponseLimitExceeded, ResponseLimits, Transport, TransportHeader, TransportMethod,
    TransportRequest, TransportResponse,
};
#[cfg(feature = "streaming")]
pub use transport::{StreamingTransport, TransportSseResponse, TransportSseStream};
pub use validation::{ValidationAdjustment, ValidationPolicy};
#[cfg(feature = "png-metadata")]
pub use vibe::VibeEncoding;
#[cfg(feature = "generation")]
pub use vibe::{EncodeVibeRequest, PreparedVibe, VibeReference, VibeTransferConfig};
