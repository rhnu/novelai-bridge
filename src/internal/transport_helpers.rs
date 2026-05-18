use secrecy::{ExposeSecret, SecretString};

use crate::{BridgeError, InvalidRequest, types::ApiKeySource};

fn validate_api_key_value(value: &str, field: &'static str) -> Result<SecretString, BridgeError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(InvalidRequest::EmptyField {
            field: field.to_owned(),
        }
        .into())
    } else {
        Ok(trimmed.to_owned().into())
    }
}

pub fn resolve_api_key(source: &ApiKeySource) -> Result<SecretString, BridgeError> {
    match source {
        ApiKeySource::Inline { value } => validate_api_key_value(value.expose_secret(), "api_key"),
        ApiKeySource::Environment => {
            let value = std::env::var("NOVELAI_API_KEY").map_err(|_| {
                InvalidRequest::MissingConfiguration {
                    name: "NOVELAI_API_KEY".to_owned(),
                }
            })?;
            validate_api_key_value(value.as_str(), "NOVELAI_API_KEY")
        }
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use crate::{BridgeError, InvalidRequest};

    use super::validate_api_key_value;

    #[test]
    fn validate_api_key_value_rejects_blank_values() {
        let error = validate_api_key_value("   ", "api_key").unwrap_err();
        assert!(matches!(
            error,
            BridgeError::InvalidRequest(InvalidRequest::EmptyField { ref field })
                if field == "api_key"
        ));
    }

    #[test]
    fn validate_api_key_value_trims_wrapping_whitespace() {
        let api_key = validate_api_key_value("  token  ", "api_key").unwrap();
        assert_eq!(api_key.expose_secret(), "token");
    }
}
