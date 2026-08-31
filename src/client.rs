use std::sync::{Arc, Mutex, atomic::AtomicU64};

use crate::{
    AnlasEstimate, AnlasPricingContext, ApiKeySource, BridgeError, CreditBalance, CreditObserver,
    DEFAULT_API_BASE, DEFAULT_IMAGE_BASE, DEFAULT_TIMEOUT_MS, GenerateImageRequest,
    ReqwestTransport, Transport, internal::transport_helpers::resolve_api_key,
};

mod director;
mod generate;
mod observe;
mod subscription;

/// Options used to construct a [`Client`] with the default [`ReqwestTransport`].
#[derive(Clone)]
pub struct ClientOptions {
    /// Selects how the API key is resolved for authenticated requests.
    pub api_key_source: ApiKeySource,
    /// Configures base URLs for `NovelAI` image and API endpoints.
    pub base_urls: BaseUrls,
    /// Overrides the HTTP timeout in milliseconds.
    pub timeout_ms: u64,
    /// Optional observer notified about request cost and account credit.
    pub credit_observer: Option<Arc<dyn CreditObserver>>,
}

impl std::fmt::Debug for ClientOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientOptions")
            .field("api_key_source", &self.api_key_source)
            .field("base_urls", &self.base_urls)
            .field("timeout_ms", &self.timeout_ms)
            .field("credit_observer", &self.credit_observer.is_some())
            .finish()
    }
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            api_key_source: ApiKeySource::Environment,
            base_urls: BaseUrls::default(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            credit_observer: None,
        }
    }
}

/// Base URLs used by [`Client`] to resolve `NovelAI` endpoint paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseUrls {
    /// Base URL for image-service endpoints, including subscription queries.
    pub image: String,
    /// Base URL reserved for non-image API endpoints.
    ///
    /// Current public client methods do not use this base URL.
    pub api: String,
}

impl Default for BaseUrls {
    fn default() -> Self {
        Self {
            image: DEFAULT_IMAGE_BASE.to_owned(),
            api: DEFAULT_API_BASE.to_owned(),
        }
    }
}

/// High-level `NovelAI` image API client.
///
/// The client performs local request-shape validation, model capability gating and image payload
/// normalization, but it intentionally does not parse or reject prompt syntax locally.
#[derive(Clone)]
pub struct Client<T: Transport> {
    #[cfg(feature = "tokenizer")]
    pub(super) tokenizers: Option<Arc<crate::Tokenizers>>,
    pub(super) transport: Arc<T>,
    pub(super) base_urls: BaseUrls,
    pub(super) credit_observer: Option<Arc<dyn CreditObserver>>,
    pub(super) request_counter: Arc<AtomicU64>,
    pub(super) last_balance: Arc<Mutex<Option<CreditBalance>>>,
}

impl Client<ReqwestTransport> {
    /// Creates a client backed by [`ReqwestTransport`].
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when the API key cannot be resolved or the default transport cannot
    /// be constructed.
    pub fn new(options: ClientOptions) -> Result<Self, BridgeError> {
        let api_key = resolve_api_key(&options.api_key_source)?;
        let transport = ReqwestTransport::new(api_key, options.timeout_ms)?;
        let client = Self::with_transport_and_base_urls(transport, options.base_urls);
        Ok(match options.credit_observer {
            Some(observer) => client.with_credit_observer(observer),
            None => client,
        })
    }
}

impl<T: Transport> Client<T> {
    /// Creates a client from an existing transport and the default base URLs.
    pub fn with_transport(transport: T) -> Self {
        Self::with_transport_and_base_urls(transport, BaseUrls::default())
    }

    /// Creates a client from an existing transport and explicit base URLs.
    ///
    /// The client resolves relative endpoint paths against the configured base URLs before
    /// calling the transport.
    pub fn with_transport_and_base_urls(transport: T, base_urls: BaseUrls) -> Self {
        Self {
            transport: Arc::new(transport),
            #[cfg(feature = "tokenizer")]
            tokenizers: None,
            base_urls,
            credit_observer: None,
            request_counter: Arc::new(AtomicU64::new(1)),
            last_balance: Arc::new(Mutex::new(None)),
        }
    }

    /// Attaches a [`CreditObserver`], replacing any previously attached observer.
    #[must_use]
    pub fn with_credit_observer(mut self, observer: Arc<dyn CreditObserver>) -> Self {
        self.credit_observer = Some(observer);
        self
    }

    /// Attaches shared local tokenizers. Generation checks each resolved prompt before transport.
    #[cfg(feature = "tokenizer")]
    #[must_use]
    pub fn with_tokenizers(mut self, tokenizers: Arc<crate::Tokenizers>) -> Self {
        self.tokenizers = Some(tokenizers);
        self
    }

    /// Counts final conditioning fields locally using model-input framing.
    ///
    /// # Errors
    /// Returns a tokenizer error if an encoder is missing or encoding fails.
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

    /// Returns the most recently sampled account balance, if [`Client::get_subscription`] has run.
    #[must_use]
    pub fn last_sampled_balance(&self) -> Option<CreditBalance> {
        self.last_balance.lock().ok().and_then(|last| last.clone())
    }

    /// Estimates a generation request with the most recently sampled account context.
    ///
    /// This is the client-facing counterpart of [`GenerateImageRequest::estimate_anlas_cost`].
    /// It never performs network I/O; when no subscription has been sampled, the estimate assumes
    /// a non-Opus account.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors.
    pub fn estimate_anlas_cost(
        &self,
        request: &GenerateImageRequest,
    ) -> Result<AnlasEstimate, BridgeError> {
        request.estimate_anlas_cost(self.last_pricing_context())
    }

    pub(super) fn last_pricing_context(&self) -> AnlasPricingContext {
        self.last_balance
            .lock()
            .ok()
            .and_then(|last| last.as_ref().map(AnlasPricingContext::from))
            .unwrap_or_default()
    }

    pub(super) fn image_endpoint(&self, path: &str) -> String {
        join_base_and_path(&self.base_urls.image, path)
    }
}

fn join_base_and_path(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}
