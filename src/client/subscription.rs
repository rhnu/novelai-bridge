use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{BridgeError, SubscriptionInfo, Transport, V5UsageStatus};

use super::Client;

impl<T: Transport> Client<T> {
    /// Fetches the current subscription summary for the configured account.
    ///
    /// Reports the result to the attached [`crate::CreditObserver`], if any.
    ///
    /// # Errors
    ///
    /// Returns [`BridgeError`] when the transport request fails or the response cannot be decoded.
    pub async fn get_subscription(&self) -> Result<SubscriptionInfo, BridgeError> {
        let endpoint = self.image_endpoint("/user/subscription");
        let raw =
            crate::protocol::get_json::<_, Value>(&*self.transport, endpoint.as_str()).await?;
        let info = map_subscription_response(&raw);
        self.observe_balance(&info).await;
        Ok(info)
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
    let subscription_active = raw
        .get("active")
        .or_else(|| raw.get("isActive"))
        .or_else(|| raw.get("is_active"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            tier > 0 && expires_at_ms.is_none_or(|expires| expires > current_time_ms())
        });

    SubscriptionInfo {
        anlas_balance,
        is_opus,
        subscription_active,
        tier,
        tier_name,
        expires_at_ms,
        v5_usage: read_v5_usage(raw),
    }
}

/// Reads the free V5 generation allowance from the `usage` object, when present.
///
/// Only `percent` decides whether an allowance is reported. `timeUntilNextPercent` is a recharge
/// rate the caller may not need, so a missing value degrades to `0` instead of discarding the
/// whole object — dropping it would also discard `isNegative`, which gates the Opus first-image
/// allowance in [`crate::estimate_anlas_cost`].
fn read_v5_usage(raw: &Value) -> Option<V5UsageStatus> {
    let usage = raw.get("usage")?;
    let percent = read_truncated_i64(usage.get("percent"))?;
    let seconds = read_truncated_i64(
        usage
            .get("timeUntilNextPercent")
            .or_else(|| usage.get("time_until_next_percent")),
    )
    .unwrap_or(0);
    let is_negative = usage
        .get("isNegative")
        .or_else(|| usage.get("is_negative"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    Some(V5UsageStatus {
        is_negative,
        percent: u32::try_from(percent.clamp(0, 100)).unwrap_or(0),
        seconds_until_next_percent: u64::try_from(seconds.max(0)).unwrap_or(0),
    })
}

/// Reads an integer, truncating fractional values the way the web client does.
fn read_truncated_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    if let Some(parsed) = read_i64(value) {
        return Some(parsed);
    }
    #[allow(clippy::cast_possible_truncation)]
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .map(|number| number.trunc() as i64)
}

fn read_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| {
            value
                .as_u64()
                .map(|item| item.min(i64::MAX.cast_unsigned()).cast_signed())
        })
        .or_else(|| {
            value
                .as_str()
                .and_then(|item| item.trim().parse::<i64>().ok())
        })
}

fn read_i32(value: Option<&Value>) -> Option<i32> {
    value
        .and_then(read_i64)
        .and_then(|item| i32::try_from(item.clamp(0, i64::from(i32::MAX))).ok())
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

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

const fn default_tier_name(tier: i32) -> &'static str {
    match tier {
        0 => "paper",
        1 => "tablet",
        2 => "scroll",
        3 => "opus",
        _ => "unknown",
    }
}
