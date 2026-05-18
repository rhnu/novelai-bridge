use std::sync::Arc;

use crate::{
    ApiKeySource, BridgeError, DEFAULT_API_BASE, DEFAULT_IMAGE_BASE, DEFAULT_TIMEOUT_MS,
    ReqwestTransport, Transport, internal::transport_helpers::resolve_api_key,
};

mod director;
mod generate;
mod subscription;

/// Options used to construct a [`Client`] with the default [`ReqwestTransport`].
#[derive(Clone, Debug)]
pub struct ClientOptions {
    /// Selects how the API key is resolved for authenticated requests.
    pub api_key_source: ApiKeySource,
    /// Configures base URLs for NovelAI image and API endpoints.
    pub base_urls: BaseUrls,
    /// Overrides the HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            api_key_source: ApiKeySource::Environment,
            base_urls: BaseUrls::default(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

/// Base URLs used by [`Client`] to resolve NovelAI endpoint paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseUrls {
    /// Base URL for image-generation endpoints.
    pub image: String,
    /// Base URL for non-image API endpoints.
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

/// High-level NovelAI image API client.
///
/// The client performs local request-shape validation and image payload normalization,
/// but it intentionally does not parse or reject prompt syntax locally.
#[derive(Clone)]
pub struct Client<T: Transport> {
    pub(super) transport: Arc<T>,
    pub(super) base_urls: BaseUrls,
}

impl Client<ReqwestTransport> {
    /// Creates a client backed by [`ReqwestTransport`].
    pub fn new(options: ClientOptions) -> Result<Self, BridgeError> {
        let api_key = resolve_api_key(&options.api_key_source)?;
        let transport = ReqwestTransport::new(api_key, options.timeout_ms)?;
        Ok(Self::with_transport_and_base_urls(
            transport,
            options.base_urls,
        ))
    }
}

impl<T: Transport> Client<T> {
    /// Creates a client from an existing transport and the default base URLs.
    pub fn with_transport(transport: T) -> Self {
        Self::with_transport_and_base_urls(transport, BaseUrls::default())
    }

    /// Creates a client from an existing transport and explicit base URLs.
    ///
    /// The client resolves relative image and API endpoint paths before calling the transport.
    pub fn with_transport_and_base_urls(transport: T, base_urls: BaseUrls) -> Self {
        Self {
            transport: Arc::new(transport),
            base_urls,
        }
    }

    pub(super) fn image_endpoint(&self, path: &str) -> String {
        join_base_and_path(&self.base_urls.image, path)
    }

    pub(super) fn api_endpoint(&self, path: &str) -> String {
        join_base_and_path(&self.base_urls.api, path)
    }
}

fn join_base_and_path(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}
