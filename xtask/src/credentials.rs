use std::{
    fs::File,
    io::{BufRead as _, BufReader},
    path::Path,
};

use anyhow::Context as _;
use novelai_bridge::{API_BASE_ENV, ApiKeySource, ClientConfig, ReqwestOptions};

use crate::TaskResult;

const API_KEY_ENV: &str = "NOVELAI_API_KEY";

pub fn client_config(explicit: Option<&str>) -> TaskResult<ClientConfig> {
    if let Some(value) = explicit {
        return Ok(ClientConfig::default().with_api_base(value)?);
    }
    if std::env::var_os(API_BASE_ENV).is_some() {
        return Ok(ClientConfig::from_env()?);
    }
    if let Some(value) = dotenv_value(API_BASE_ENV)? {
        return Ok(ClientConfig::default().with_api_base(&value)?);
    }
    Ok(ClientConfig::default())
}

pub fn options() -> TaskResult<ReqwestOptions> {
    let mut options = ReqwestOptions::default();
    if std::env::var_os(API_KEY_ENV).is_some() {
        return Ok(options);
    }
    let value = dotenv_value(API_KEY_ENV)?
        .context("set NOVELAI_API_KEY in the process environment or repository .env")?;
    options.api_key_source = ApiKeySource::Inline {
        value: value.into(),
    };
    Ok(options)
}

fn dotenv_value(name: &str) -> TaskResult<Option<String>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("open {}", path.display())),
    };
    for line in BufReader::new(file).lines() {
        let line = line.with_context(|| format!("read {}", path.display()))?;
        if let Some(value) = parse_value(&line, name) {
            return Ok(Some(value.to_owned()));
        }
    }
    Ok(None)
}

fn parse_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (key, value) = line.trim().trim_start_matches("export ").split_once('=')?;
    if key.trim() != name {
        return None;
    }
    let value = value.trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        });
    Some(unquoted.unwrap_or(value))
}
