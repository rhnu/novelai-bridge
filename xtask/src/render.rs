//! Human-readable rendering of request and response payloads.
//!
//! Request bodies carry multi-megabyte base64 image strings. Printing them raw makes the output
//! useless, so long strings are elided down to their decoded size.

use serde_json::{Map, Value};

/// Strings longer than this are candidates for eliding.
const ELIDE_ABOVE_CHARS: usize = 128;

/// Renders a request body with long base64 payloads elided.
pub fn to_readable_json(value: &Value) -> String {
    serde_json::to_string_pretty(&elide(value)).unwrap_or_else(|_| value.to_string())
}

/// Only base64-looking blobs are elided. Prompt text stays readable however long it is, because
/// verifying prompt and UC preset assembly is most of what this tool is for.
fn is_payload(text: &str) -> bool {
    text.len() > ELIDE_ABOVE_CHARS
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'=' | b'-'))
}

fn elide(value: &Value) -> Value {
    match value {
        Value::String(text) if is_payload(text) => Value::String(format!(
            "<base64, {} chars, ~{} KiB decoded, starts {}…>",
            text.len(),
            text.len() / 4 * 3 / 1024,
            &text[..24]
        )),
        Value::Array(items) => Value::Array(items.iter().map(elide).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), elide(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Sets a dotted path such as `parameters.add_original_image` to `replacement`.
///
/// Numeric segments index into arrays. Returns `false` when the path does not resolve, so the
/// caller can warn instead of silently doing nothing.
pub fn set_path(root: &mut Value, path: &str, replacement: Value) -> bool {
    let mut segments = path.split('.').peekable();
    let mut cursor = root;

    while let Some(segment) = segments.next() {
        let is_last = segments.peek().is_none();
        match cursor {
            Value::Object(entries) => {
                if is_last {
                    entries.insert(segment.to_owned(), replacement);
                    return true;
                }
                let Some(next) = entries.get_mut(segment) else {
                    return false;
                };
                cursor = next;
            },
            Value::Array(items) => {
                let Ok(index) = segment.parse::<usize>() else {
                    return false;
                };
                let Some(next) = items.get_mut(index) else {
                    return false;
                };
                if is_last {
                    *next = replacement;
                    return true;
                }
                cursor = next;
            },
            _ => return false,
        }
    }

    false
}

/// Removes a dotted path. Returns `false` when the path does not resolve.
pub fn remove_path(root: &mut Value, path: &str) -> bool {
    let Some((parent_path, leaf)) = path.rsplit_once('.') else {
        return match root {
            Value::Object(entries) => entries.remove(path).is_some(),
            _ => false,
        };
    };

    let mut cursor = root;
    for segment in parent_path.split('.') {
        cursor = match cursor {
            Value::Object(entries) => match entries.get_mut(segment) {
                Some(next) => next,
                None => return false,
            },
            Value::Array(items) => {
                match segment.parse::<usize>().ok().and_then(|i| items.get_mut(i)) {
                    Some(next) => next,
                    None => return false,
                }
            },
            _ => return false,
        };
    }

    match cursor {
        Value::Object(entries) => entries.remove(leaf).is_some(),
        Value::Array(items) => leaf
            .parse::<usize>()
            .ok()
            .filter(|index| *index < items.len())
            .map(|index| items.remove(index))
            .is_some(),
        _ => false,
    }
}

/// Parses a `path=value` override. The value is JSON when it parses, otherwise a plain string.
pub fn parse_override(raw: &str) -> Result<(String, Value), String> {
    let (path, value) = raw
        .split_once('=')
        .ok_or_else(|| format!("--set expects path=value, got `{raw}`"))?;
    if path.is_empty() {
        return Err(format!("--set path cannot be empty in `{raw}`"));
    }
    let parsed = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_owned()));
    Ok((path.to_owned(), parsed))
}

/// Renders a subscription response, keeping account identifiers out of the output.
pub fn subscription_summary(info: &novelai_bridge::SubscriptionInfo) -> String {
    let mut summary = Map::new();
    summary.insert("anlas_balance".into(), info.anlas_balance.into());
    summary.insert("is_opus".into(), info.is_opus().into());
    summary.insert("tier".into(), info.tier.into());
    summary.insert("tier_name".into(), info.tier_name().into());
    summary.insert(
        "v5_usage".into(),
        info.v5_usage.map_or(Value::Null, |usage| {
            let mut entry = Map::new();
            entry.insert("is_negative".into(), usage.is_negative.into());
            entry.insert("percent".into(), usage.percent.into());
            entry.insert(
                "seconds_until_next_percent".into(),
                usage.seconds_until_next_percent.into(),
            );
            Value::Object(entry)
        }),
    );
    serde_json::to_string_pretty(&Value::Object(summary)).unwrap_or_default()
}
