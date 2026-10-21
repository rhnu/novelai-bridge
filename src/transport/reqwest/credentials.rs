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
        ApiKeySource::Environment => std::env::var("NOVELAI_API_KEY")
            .map_err(|source| {
                TransportError::new(
                    TransportOperation::ResolveCredentials,
                    None,
                    Box::new(source),
                )
            })?
            .into(),
    };
    let value = secret.expose_secret().trim();
    if value.is_empty() {
        return Err(TransportError::new(
            TransportOperation::ResolveCredentials,
            None,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "API key cannot be blank",
            )),
        ));
    }
    if value.len() == secret.expose_secret().len() {
        Ok(secret)
    } else {
        Ok(value.to_owned().into())
    }
}

#[cfg(test)]
mod tests;
