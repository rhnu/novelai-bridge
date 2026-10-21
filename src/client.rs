use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    AnlasEstimate, AnlasPricingContext, BridgeError, GenerateImageRequest, SubscriptionInfo,
};

mod diagnostics;
mod director;
mod execute;
mod generate;
mod image_request;
mod observe;
mod stream;
mod subscription;
mod tags;
mod upscale;
mod vibe;

pub use image_request::{ImageCacheConfig, ImageRequestEncoding};
pub use stream::{GenerateImageStream, ImageStreamItem, ImageStreamResult};

/// Default base URL for image-service endpoints.
pub const DEFAULT_IMAGE_BASE: &str = "https://image.novelai.net";

/// Service routing shared by all transport implementations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientConfig {
    pub image_base_url: String,
    /// Response budgets applied to every transport implementation.
    pub response_limits: crate::ResponseLimits,
    /// Encoding for image-service POST bodies. JSON retains the original base64 representation.
    pub image_request_encoding: ImageRequestEncoding,
    /// Optional bounded server image-cache registration shared by cloned clients.
    pub image_cache: Option<ImageCacheConfig>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            image_base_url: DEFAULT_IMAGE_BASE.to_owned(),
            response_limits: crate::ResponseLimits::default(),
            image_request_encoding: ImageRequestEncoding::default(),
            image_cache: None,
        }
    }
}

/// Image API client with shared transport and sampled account state.
pub struct Client<T: ?Sized> {
    transport: Arc<T>,
    config: ClientConfig,
    account: Arc<Mutex<Option<SubscriptionInfo>>>,
    request_counter: Arc<AtomicU64>,
    image_cache: Arc<Mutex<image_request::CacheState>>,
    #[cfg(feature = "tokenizer")]
    tokenizers: Option<Arc<crate::Tokenizers>>,
    observation: Option<observe::ObservationContext>,
}

impl<T: ?Sized> Clone for Client<T> {
    fn clone(&self) -> Self {
        Self {
            transport: Arc::clone(&self.transport),
            config: self.config.clone(),
            account: Arc::clone(&self.account),
            request_counter: Arc::clone(&self.request_counter),
            image_cache: Arc::clone(&self.image_cache),
            #[cfg(feature = "tokenizer")]
            tokenizers: self.tokenizers.clone(),
            observation: self.observation.clone(),
        }
    }
}

impl<T: ?Sized> Client<T> {
    /// Creates a client from a shared transport and service configuration.
    pub fn new(transport: Arc<T>, config: ClientConfig) -> Self {
        Self {
            transport,
            config,
            account: Arc::new(Mutex::new(None)),
            request_counter: Arc::new(AtomicU64::new(1)),
            image_cache: Arc::default(),
            #[cfg(feature = "tokenizer")]
            tokenizers: None,
            observation: None,
        }
    }

    /// Attaches a credit observer. Clones share request identifiers and the observer.
    #[must_use]
    pub fn with_credit_observer(mut self, observer: Arc<dyn crate::CreditObserver>) -> Self {
        self.observation = Some(observe::ObservationContext::new(observer));
        self
    }

    /// Attaches local tokenizers used to check final conditioning fields.
    #[cfg(feature = "tokenizer")]
    #[must_use]
    pub fn with_tokenizers(mut self, tokenizers: Arc<crate::Tokenizers>) -> Self {
        self.tokenizers = Some(tokenizers);
        self
    }

    /// Counts final conditioning fields using model-input framing.
    ///
    /// # Errors
    /// Returns an error when an encoder is missing or encoding fails.
    #[cfg(feature = "tokenizer")]
    pub fn count_tokens(
        &self,
        request: &GenerateImageRequest,
    ) -> Result<crate::PromptTokenUsage, BridgeError> {
        let tokenizers =
            self.tokenizers
                .as_ref()
                .ok_or_else(|| crate::TokenizerError::TokenizerNotLoaded {
                    kind: request.descriptor().prompt().tokenizer,
                })?;
        request.count_tokens(tokenizers)
    }

    /// Returns the most recently sampled subscription, shared by client clones.
    #[must_use]
    pub fn last_subscription(&self) -> Option<SubscriptionInfo> {
        self.account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Estimates generation using the most recently sampled account context.
    ///
    /// # Errors
    /// Returns local normalization errors.
    pub fn estimate_anlas_cost(
        &self,
        request: &GenerateImageRequest,
    ) -> Result<AnlasEstimate, BridgeError> {
        request.estimate_anlas_cost(self.last_pricing_context())
    }

    fn last_pricing_context(&self) -> AnlasPricingContext {
        self.last_subscription()
            .as_ref()
            .map_or_else(AnlasPricingContext::default, AnlasPricingContext::from)
    }

    fn next_request_id(&self) -> u64 { self.request_counter.fetch_add(1, Ordering::Relaxed) }

    fn request_span(operation: &'static str, request_id: u64) -> tracing::Span {
        tracing::info_span!(
            "novelai.request",
            operation,
            request_id,
            endpoint = tracing::field::Empty,
            model = tracing::field::Empty,
            tool = tracing::field::Empty
        )
    }
}

#[cfg(test)]
mod tests;
