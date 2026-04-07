use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use secrecy::SecretString;

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
    /// Overrides the base URL used by image-generation endpoints.
    pub image_base: Option<String>,
    /// Overrides the base URL used by non-image API endpoints such as subscription lookup.
    pub api_base: Option<String>,
    /// Overrides the HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            api_key_source: ApiKeySource::Environment,
            image_base: Some(DEFAULT_IMAGE_BASE.to_owned()),
            api_base: Some(DEFAULT_API_BASE.to_owned()),
            timeout_ms: DEFAULT_TIMEOUT_MS,
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
    pub(super) api_base: String,
    pub(super) vibe_cache: Arc<Mutex<HashMap<String, String>>>,
}

impl Client<ReqwestTransport> {
    /// Creates a client backed by [`ReqwestTransport`].
    pub fn new(options: ClientOptions) -> Result<Self, BridgeError> {
        let api_key = resolve_api_key(&options.api_key_source)?;
        Self::from_parts(api_key, options)
    }

    fn from_parts(api_key: SecretString, options: ClientOptions) -> Result<Self, BridgeError> {
        let image_base = options
            .image_base
            .unwrap_or_else(|| DEFAULT_IMAGE_BASE.to_owned());
        let api_base = options
            .api_base
            .unwrap_or_else(|| DEFAULT_API_BASE.to_owned());
        let transport = ReqwestTransport::new(api_key, image_base, options.timeout_ms)?;
        Ok(Self::with_transport_and_api_base(transport, api_base))
    }
}

impl<T: Transport> Client<T> {
    /// Creates a client from an existing transport and the default API base.
    pub fn with_transport(transport: T) -> Self {
        Self::with_transport_and_api_base(transport, DEFAULT_API_BASE.to_owned())
    }

    /// Creates a client from an existing transport and an explicit API base.
    ///
    /// The provided `api_base` is used for non-image endpoints such as subscription lookup.
    /// Relative image endpoints are still resolved by the transport implementation.
    pub fn with_transport_and_api_base(transport: T, api_base: String) -> Self {
        Self {
            transport: Arc::new(transport),
            api_base,
            vibe_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
