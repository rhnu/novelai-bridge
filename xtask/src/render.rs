//! Human-readable rendering of request and response payloads.
//!
//! Request bodies carry multi-megabyte base64 image strings. Printing them raw makes the output
//! useless, so long strings are elided down to their decoded size.

use serde_json::{Map, Value};

/// Strings longer than this are candidates for eliding.
const ELIDE_ABOVE_CHARS: usize = 128;

/// Renders a request body with long base64 payloads elided.
pub fn to_readable_json(value: &Value) -> String {
    serde_json::to_string_pretty(&elide(value))
        .expect("serializing a JSON Value to a String cannot fail")
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
    let Some((parent, leaf)) = parent_mut(root, path) else {
        return false;
    };
    match parent {
        Value::Object(entries) => {
            entries.insert(leaf.to_owned(), replacement);
            true
        },
        Value::Array(items) => {
            let Some(slot) = leaf
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get_mut(index))
            else {
                return false;
            };
            *slot = replacement;
            true
        },
        _ => false,
    }
}

/// Removes a dotted path. Missing parents and removal targets are errors to the caller.
pub fn remove_path(root: &mut Value, path: &str) -> bool {
    let Some((parent, leaf)) = parent_mut(root, path) else {
        return false;
    };
    match parent {
        Value::Object(entries) => entries.remove(leaf).is_some(),
        Value::Array(items) => {
            let Some(index) = leaf
                .parse::<usize>()
                .ok()
                .filter(|index| *index < items.len())
            else {
                return false;
            };
            items.remove(index);
            true
        },
        _ => false,
    }
}

fn parent_mut<'value, 'path>(
    root: &'value mut Value,
    path: &'path str,
) -> Option<(&'value mut Value, &'path str)> {
    let (parent, leaf) = path
        .rsplit_once('.')
        .map_or((None, path), |(parent, leaf)| (Some(parent), leaf));
    if leaf.is_empty() {
        return None;
    }
    let mut cursor = root;
    if let Some(parent) = parent {
        for segment in parent.split('.') {
            if segment.is_empty() {
                return None;
            }
            cursor = match cursor {
                Value::Object(entries) => entries.get_mut(segment)?,
                Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
    }
    Some((cursor, leaf))
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
    serde_json::to_string_pretty(&Value::Object(summary))
        .expect("serializing a JSON Value to a String cannot fail")
}

#[cfg(test)]
mod tests;
