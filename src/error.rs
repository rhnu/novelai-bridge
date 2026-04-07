use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Stable error codes returned by this crate.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BridgeErrorCode {
    /// The request failed local validation or the API rejected its shape.
    InvalidRequest,
    /// Authentication failed, usually because the API key is missing or invalid.
    AuthenticationFailed,
    /// The account does not have enough credit or subscription access.
    InsufficientCredit,
    /// The request conflicted with the current server-side state.
    RequestConflict,
    /// The request was rate limited.
    RateLimited,
    /// A transport-level network error occurred.
    NetworkError,
    /// The server returned a 5xx response.
    ServerError,
    /// The error did not match a more specific category.
    UnknownError,
}

impl BridgeErrorCode {
    /// Returns the serialized string form used by [`BridgeError::code`].
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::AuthenticationFailed => "authentication_failed",
            Self::InsufficientCredit => "insufficient_credit",
            Self::RequestConflict => "request_conflict",
            Self::RateLimited => "rate_limited",
            Self::NetworkError => "network_error",
            Self::ServerError => "server_error",
            Self::UnknownError => "unknown_error",
        }
    }
}

/// Unified error type returned by this crate.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BridgeError {
    /// Machine-readable error code.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// Parsed retry delay in milliseconds when available.
    pub retry_after_ms: Option<u64>,
    /// Optional structured details attached by the caller or mapper.
    pub details: Option<Value>,
}

impl BridgeError {
    /// Creates a new error with the given code and message.
    pub fn new(code: BridgeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str().to_owned(),
            message: message.into(),
            retry_after_ms: None,
            details: None,
        }
    }

    /// Attaches a parsed retry delay in milliseconds.
    pub fn with_retry_after_ms(mut self, retry_after_ms: Option<u64>) -> Self {
        self.retry_after_ms = retry_after_ms;
        self
    }

    /// Attaches structured JSON details.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Creates an `invalid_request` error.
    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::InvalidRequest, message)
    }

    /// Creates an `authentication_failed` error.
    pub fn authentication_failed(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::AuthenticationFailed, message)
    }

    /// Creates an `insufficient_credit` error.
    pub fn insufficient_credit(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::InsufficientCredit, message)
    }

    /// Creates a `request_conflict` error.
    pub fn request_conflict(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::RequestConflict, message)
    }

    /// Creates a `rate_limited` error with an optional retry delay.
    pub fn rate_limited(message: impl Into<String>, retry_after_ms: Option<u64>) -> Self {
        Self::new(BridgeErrorCode::RateLimited, message).with_retry_after_ms(retry_after_ms)
    }

    /// Creates a `network_error` error.
    pub fn network_error(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::NetworkError, message)
    }

    /// Creates a `server_error` error.
    pub fn server_error(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::ServerError, message)
    }

    /// Creates an `unknown_error` error.
    pub fn unknown_error(message: impl Into<String>) -> Self {
        Self::new(BridgeErrorCode::UnknownError, message)
    }
}

impl core::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for BridgeError {}
