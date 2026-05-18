use serde_json::Value;

use crate::{BridgeError, SubscriptionInfo, Transport};

use super::Client;

impl<T: Transport> Client<T> {
    /// Fetches the current subscription summary for the configured account.
    pub async fn get_subscription(&self) -> Result<SubscriptionInfo, BridgeError> {
        let endpoint = self.api_endpoint("/user/subscription");
        let raw = self.transport.get_json::<Value>(endpoint.as_str()).await?;
        Ok(map_subscription_response(&raw))
    }
}

fn map_subscription_response(raw: &Value) -> SubscriptionInfo {
    let tier = read_i32(raw.get("tier")).unwrap_or(0);
    let tier_name = raw
        .get("tier_name")
        .or_else(|| raw.get("tierName"))
        .and_then(Value::as_str)
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| default_tier_name(tier).to_owned());
    let is_opus = tier >= 3 || tier_name == "opus";

    let training = raw
        .get("trainingStepsLeft")
        .or_else(|| raw.get("training_steps_left"));
    let fixed = training
        .and_then(|value| {
            value
                .get("fixedTrainingStepsLeft")
                .or_else(|| value.get("fixed_training_steps_left"))
        })
        .and_then(read_i64)
        .unwrap_or(0);
    let purchased = training
        .and_then(|value| {
            value
                .get("purchasedTrainingSteps")
                .or_else(|| value.get("purchased_training_steps"))
        })
        .and_then(read_i64)
        .unwrap_or(0);
    let anlas_balance = fixed.saturating_add(purchased).max(0);
    let expires_at_ms = raw
        .get("expiresAt")
        .or_else(|| raw.get("expires_at"))
        .and_then(read_timestamp_ms);

    SubscriptionInfo {
        anlas_balance,
        is_opus,
        tier,
        tier_name,
        expires_at_ms,
    }
}

fn read_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|item| item.min(i64::MAX as u64) as i64))
        .or_else(|| {
            value
                .as_str()
                .and_then(|item| item.trim().parse::<i64>().ok())
        })
}

fn read_i32(value: Option<&Value>) -> Option<i32> {
    value
        .and_then(read_i64)
        .map(|item| item.clamp(0, i32::MAX as i64) as i32)
}

fn read_timestamp_ms(value: &Value) -> Option<u64> {
    let raw = value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|item| u64::try_from(item).ok()))
        .or_else(|| {
            value
                .as_str()
                .and_then(|item| item.trim().parse::<u64>().ok())
        })?;

    if raw < 1_000_000_000_000 {
        Some(raw.saturating_mul(1000))
    } else {
        Some(raw)
    }
}

fn default_tier_name(tier: i32) -> &'static str {
    match tier {
        0 => "paper",
        1 => "tablet",
        2 => "scroll",
        3 => "opus",
        _ => "unknown",
    }
}
