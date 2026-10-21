use std::{error::Error as StdError, fmt, time::Duration};

/// Non-success response returned by the `NovelAI` API.
#[derive(Debug)]
#[non_exhaustive]
pub struct ApiError {
    /// Category derived from the HTTP status code.
    pub kind: ApiErrorKind,
    /// HTTP status code returned by the API.
    pub status: u16,
    /// Full endpoint URL used for the request.
    pub endpoint: String,
    /// Parsed reason extracted from known API error payload shapes.
    pub server_reason: Option<ApiErrorReason>,
    /// Raw response body text when present.
    pub raw_body: Option<String>,
}

impl ApiError {
    /// Constructs an API failure; optional response details may be assigned through public fields.
    #[must_use]
    pub fn new(kind: ApiErrorKind, status: u16, endpoint: impl Into<String>) -> Self {
        Self {
            kind,
            status,
            endpoint: endpoint.into(),
            server_reason: None,
            raw_body: None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.server_reason {
            Some(reason) => write!(
                f,
                "{}: {} from {} (HTTP {})",
                self.kind, reason, self.endpoint, self.status
            ),
            None => write!(
                f,
                "{} from {} (HTTP {})",
                self.kind, self.endpoint, self.status
            ),
        }
    }
}

impl StdError for ApiError {}

/// API error category derived from HTTP status and headers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorKind {
    /// HTTP 400.
    InvalidRequest,
    /// HTTP 401.
    AuthenticationFailed,
    /// HTTP 402.
    InsufficientCredit,
    /// HTTP 409.
    RequestConflict,
    /// HTTP 429 with optional parsed retry delay.
    RateLimited { retry_after: Option<Duration> },
    /// HTTP 5xx.
    ServerError,
    /// Any status without a specific category.
    UnexpectedStatus,
}

impl fmt::Display for ApiErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest => f.write_str("API rejected the request"),
            Self::AuthenticationFailed => f.write_str("API authentication failed"),
            Self::InsufficientCredit => {
                f.write_str("API reported insufficient credit or subscription access")
            }
            Self::RequestConflict => f.write_str("API reported a request conflict"),
            Self::RateLimited { retry_after } => {
                if let Some(delay) = retry_after {
                    write!(f, "API rate limited the request; retry after {delay:?}")
                } else {
                    f.write_str("API rate limited the request")
                }
            }
            Self::ServerError => f.write_str("API returned a server error"),
            Self::UnexpectedStatus => f.write_str("API returned an unexpected status"),
        }
    }
}

/// Parsed reason extracted from a known API error body field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorReason {
    /// Top-level `message`.
    Message(String),
    /// Top-level `detail`.
    Detail(String),
    /// Nested `error.message`.
    ErrorMessage(String),
}

impl ApiErrorReason {
    /// Returns the extracted reason text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Message(value) | Self::Detail(value) | Self::ErrorMessage(value) => value,
        }
    }
}

impl fmt::Display for ApiErrorReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
