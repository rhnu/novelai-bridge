mod client;
mod director;
mod error;
mod field;
mod generation;
mod image;
mod media_policy;
mod metadata;
mod model;
mod observation;
mod parameters;
mod payload;
mod pricing;
mod protocol;
mod stream;
mod subscription;
mod tag;
#[cfg(feature = "tokenizer")]
mod tokenizer;
mod transport;
mod upscale;
mod validation;
mod vibe;

pub use client::{
    Client, ClientConfig, DEFAULT_IMAGE_BASE, GenerateImageStream, ImageCacheConfig,
    ImageRequestEncoding, ImageStreamItem, ImageStreamResult,
};
pub use director::{
    DirectorCommand, PreparedDirector, RunDirectorToolRequest, RunDirectorToolResult,
};
pub use error::{
    ApiError, ApiErrorKind, ApiErrorReason, BridgeError, DataUrlError, DecodeError, DecodeTarget,
    EncodeError, EncodeTarget, ImageStreamError, InvalidRequest, MetadataError, PreparationError,
    TransportError, TransportOperation,
};
pub use field::{
    CharacterField, CharacterReferenceField, GenerationField, GuidanceField, ImageInputField,
    InpaintImageToImageField, RequestField, VibeReferenceField,
};
pub use generation::{
    Character, CharacterPosition, CharacterReference, CharacterReferenceType, CharacterSet,
    GenerateImageRequest, GenerateImageRequestBuilder, GenerateImageResult,
    GenerateImageStreamRequest, GenerationCore, GenerationDelivery, GenerationGeometry,
    GenerationInput, GuidanceParams, ImageToImageInput, InpaintImageToImage, InpaintingInput,
    NormalizationChange, NormalizationDiagnostic, NormalizedGeneration, OutputParams,
    PreparedGeneration, ReferenceSet, ResolvedCharacterPrompt, ResolvedPrompts,
};
#[cfg(feature = "tokenizer")]
pub use generation::{CharacterTokenUsage, PromptField, PromptTokenUsage};
pub use image::{
    DefaultImagePreprocessor, EncodedImage, GeneratedImage, GeneratedImageSource, ImageMediaFormat,
    ImagePreparation, ImagePreprocessor, PreparedImage, decode_generated_image_response,
    decode_generated_image_response_with_policy, decode_generated_images,
    decode_generated_images_with_policy,
};
pub use media_policy::{ImageDecodePolicy, ImageLimit, ImagePolicyError};
pub use metadata::{
    CharacterMetadata, GeneratedImageMetadata, ImageMetadata, MetadataField, MetadataOrigin,
    MetadataWarning, VibeMetadataEntry, VibeMetadataSource,
    extract_embedded_vibe_document_from_png_bytes, inspect_generated_image_metadata,
    parse_image_metadata_from_bytes, parse_image_metadata_from_bytes_with_policy,
    parse_png_metadata_from_bytes, parse_png_metadata_from_bytes_with_policy,
    parse_png_metadata_from_payload,
    png::{PngTextEntry, write_png_text_chunks, write_png_text_chunks_with_policy},
};
pub use model::{
    CanvasConstraints, CharacterPositionProfile, CharacterPromptProfile, GenerationConstraints,
    GenerationMode, ImageTokenizerKind, InpaintingProfile, Model, ModelDefaults, ModelDescriptor,
    ModelIdentity, ModelSupport, NoiseScheduleSupport, OpusAllowance, ParameterRange,
    PricingProfile, PromptProfile, PromptStructure, QualityPresetDefinition, ReferenceAvailability,
    ReferenceIntent, ReferenceModes, ReferenceProfile, ReferenceResolution, SamplerCompatibility,
    SamplingProfile, UcPresetDefinition, VarietyBoostProfile, VibeInputPath, VibeProfile,
    WireProfile,
};
pub use novelai_bridge_prompt::{
    PromptAnalysis, PromptDiagnostic, PromptDiagnosticCode, PromptDiagnosticSeverity,
    PromptLexicalState, PromptSemanticKind, PromptSemanticSpan, PromptSyntaxProfile, PromptToken,
    PromptTokenKind, PromptWeight, analyze_prompt, is_character_separator, is_randomizer_delimiter,
    is_text_rendering_marker,
};
pub use observation::{
    CreditEvent, CreditObserver, GenerationSummary, ObservedDirectorTool, ObservedOperation,
    RequestCostEstimate, RequestFailure, RequestFingerprint, RequestOutcome,
};
pub use parameters::{
    AlphaMode, ImageFormat, ImageResponseFormat, ImageSize, NoiseSchedule, QualityPreset, Sampler,
    SamplingParams, StreamMode, UcPreset,
};
pub use pricing::{
    AnlasEstimate, AnlasEstimateInput, AnlasEstimateStatus, AnlasPricingContext, PricingMode,
    estimate_anlas_cost, estimate_director_tool_cost,
};
pub use protocol::{ApiEndpoint, BufferedResponse};
#[cfg(feature = "reqwest-transport")]
pub use secrecy::SecretString;
pub use stream::{
    ImageStreamChunk, ImageStreamEvent, ImageStreamEventType, StreamEventSource, TransportSseEvent,
};
pub use subscription::{SubscriptionInfo, V5UsageStatus};
pub use tag::{
    JapaneseTagSuggestion, SuggestTagsRequest, SuggestTagsResult, TagLanguage, TagSuggestion,
};
#[cfg(feature = "tokenizer")]
pub use tokenizer::{
    ClipTokenizer, FramingPolicy, QwenTokenizer, T5Tokenizer, TokenCount, TokenEncoder, TokenId,
    TokenizerAsset, TokenizerError, Tokenizers,
};
#[cfg(feature = "reqwest-transport")]
pub use transport::{ApiKeySource, DEFAULT_TIMEOUT_MS, ReqwestOptions, ReqwestTransport};
pub use transport::{
    ResponseLimitExceeded, ResponseLimits, Transport, TransportByteStream, TransportHeader,
    TransportMethod, TransportRequest, TransportResponse,
};
pub use upscale::{
    PreparedUpscale, UpscaleBlurSigma, UpscaleImageRequest, UpscaleImageResult,
    estimate_upscale_cost,
};
pub use validation::{ValidationAdjustment, ValidationPolicy};
pub use vibe::{
    EncodeVibeRequest, PreparedVibe, VibeBundle, VibeDocument, VibeDocumentError,
    VibeDocumentLimits, VibeDocumentResource, VibeEncoding, VibeEncodingParams, VibeEncodingRecord,
    VibeFile, VibeFileKind, VibeImportInfo, VibeReference, VibeReferenceInput, VibeTransferConfig,
};
