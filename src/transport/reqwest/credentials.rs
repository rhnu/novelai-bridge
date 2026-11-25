use secrecy::{ExposeSecret, SecretString};

use crate::{TransportError, TransportOperation};

/// How the reqwest backend resolves its authentication key.
#[derive(Clone, Debug)]
pub enum ApiKeySource {
    Environment,
    Inline { value: SecretString },
}

pub(super) fn resolve(source: ApiKeySource) -> Result<SecretString, TransportError> {
    let secret = match source {
        ApiKeySource::Inline { value } => value,
        ApiKeySource::Environment => match std::env::var("NOVELAI_API_KEY") {
            Ok(value) => value.into(),
            Err(std::env::VarError::NotPresent) => {
                return Err(credential_error(
                    std::io::ErrorKind::NotFound,
                    "NOVELAI_API_KEY is unset",
                ));
            },
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(credential_error(
                    std::io::ErrorKind::InvalidData,
                    "NOVELAI_API_KEY must be valid Unicode",
                ));
            },
        },
    };
    let value = secret.expose_secret().trim();
    if value.is_empty() {
        return Err(credential_error(
            std::io::ErrorKind::InvalidInput,
            "API key cannot be blank",
        ));
    }
    if value.len() == secret.expose_secret().len() {
        Ok(secret)
    } else {
        Ok(value.to_owned().into())
    }
}

fn credential_error(kind: std::io::ErrorKind, message: &'static str) -> TransportError {
    TransportError::new(
        TransportOperation::ResolveCredentials,
        None,
        Box::new(std::io::Error::new(kind, message)),
    )
}

#[cfg(test)]
mod tests;
