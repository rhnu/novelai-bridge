use crate::{TransportMethod, TransportRequest};

/// One supported operation and its HTTP request description.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ApiEndpoint {
    GenerateImage,
    GenerateImageStream,
    EncodeVibe,
    AugmentImage,
    Subscription,
}

impl ApiEndpoint {
    pub const fn path(self) -> &'static str {
        match self {
            Self::GenerateImage => "/ai/generate-image",
            Self::GenerateImageStream => "/ai/generate-image-stream",
            Self::EncodeVibe => "/ai/encode-vibe",
            Self::AugmentImage => "/ai/augment-image",
            Self::Subscription => "/user/subscription",
        }
    }

    #[must_use]
    pub const fn method(self) -> TransportMethod {
        match self {
            Self::Subscription => TransportMethod::Get,
            _ => TransportMethod::Post,
        }
    }

    #[must_use]
    pub const fn accept(self) -> &'static str {
        match self {
            Self::Subscription => "application/json",
            Self::GenerateImageStream => "text/event-stream",
            _ => "application/octet-stream",
        }
    }

    #[must_use]
    pub const fn content_type(self) -> Option<&'static str> {
        match self {
            Self::Subscription => None,
            _ => Some("application/json"),
        }
    }

    /// Builds the same request used by the client and offline inspection tools.
    #[must_use]
    pub fn request(self, base_url: &str, body: Option<Vec<u8>>) -> TransportRequest {
        TransportRequest {
            method: self.method(),
            endpoint: format!("{}{}", base_url.trim_end_matches('/'), self.path()),
            accept: Some(self.accept().to_owned()),
            content_type: self.content_type().map(str::to_owned),
            body,
        }
    }
}

#[cfg(test)]
mod tests;
