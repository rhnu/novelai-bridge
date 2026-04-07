use futures_util::StreamExt;
use secrecy::SecretString;

use crate::{
    BridgeError, ImageStreamChunk, ImageStreamResult, transport::TransportSseStream,
    types::ApiKeySource,
};

fn validate_api_key_value(
    value: &str,
    empty_message: &'static str,
) -> Result<SecretString, BridgeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(BridgeError::authentication_failed(empty_message))
    } else {
        Ok(trimmed.to_owned().into())
    }
}

pub(crate) fn resolve_api_key(source: &ApiKeySource) -> Result<SecretString, BridgeError> {
    match source {
        ApiKeySource::Inline { value } => {
            validate_api_key_value(value, "Inline API key cannot be empty.")
        }
        ApiKeySource::Environment => {
            let value = std::env::var("NOVELAI_API_KEY").map_err(|_| {
                BridgeError::authentication_failed("NOVELAI_API_KEY is not configured.")
            })?;
            validate_api_key_value(value.as_str(), "NOVELAI_API_KEY is configured but empty.")
        }
    }
}

pub(crate) fn map_sse_stream(stream: TransportSseStream) -> ImageStreamResult {
    let mapped = stream.map(|item| match item {
        Ok(line) => ImageStreamChunk::from_sse_line(&line).map_err(|err| {
            BridgeError::unknown_error(format!("Failed to parse stream chunk: {err}"))
        }),
        Err(err) => Err(err),
    });
    Box::pin(mapped)
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::validate_api_key_value;

    #[test]
    fn validate_api_key_value_rejects_blank_values() {
        let error = validate_api_key_value("   ", "blank").unwrap_err();
        assert_eq!(error.code, "authentication_failed");
        assert_eq!(error.message, "blank");
    }

    #[test]
    fn validate_api_key_value_trims_wrapping_whitespace() {
        let api_key = validate_api_key_value("  token  ", "blank").unwrap();
        assert_eq!(api_key.expose_secret(), "token");
    }
}
