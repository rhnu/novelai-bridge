use secrecy::ExposeSecret;

use super::{ApiKeySource, resolve};
use crate::{ReqwestOptions, TransportOperation};

#[test]
fn inline_credentials_are_trimmed_and_blank_values_are_rejected() {
    let secret = resolve(ApiKeySource::Inline {
        value: " token ".into(),
    })
    .unwrap();
    assert_eq!(secret.expose_secret(), "token");
    let error = resolve(ApiKeySource::Inline { value: " ".into() }).unwrap_err();
    assert_eq!(error.operation, TransportOperation::ResolveCredentials);
}

#[test]
fn backend_options_do_not_expose_inline_credentials() {
    let options = ReqwestOptions {
        api_key_source: ApiKeySource::Inline {
            value: "secret-token".into(),
        },
        ..ReqwestOptions::default()
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("secret-token"));
    assert!(debug.contains("REDACTED"));
}
