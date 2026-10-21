use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod wire;

pub use wire::SubscriptionResponse;

/// V5 generation allowance reported by `/user/subscription`.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct V5UsageStatus {
    /// Whether the allowance is currently overdrawn.
    pub is_negative: bool,
    /// Remaining allowance as an integer percentage, which may exceed 100.
    pub percent: u32,
    /// Purchased allowance as an integer percentage; zero when omitted.
    #[serde(default)]
    pub purchased_percent: u32,
    /// Seconds required to recharge one percent.
    pub seconds_until_next_percent: u64,
}

/// Subscription summary returned by [`crate::Client::get_subscription`].
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionInfo {
    /// Combined fixed and purchased Anlas balance.
    pub anlas_balance: i64,
    /// Whether the paid subscription is currently active.
    pub subscription_active: bool,
    /// Numeric tier value.
    pub tier: i32,
    /// Optional expiration timestamp in milliseconds since the Unix epoch.
    pub expires_at_ms: Option<u64>,
    /// V5 generation allowance, when the account reports one.
    pub v5_usage: Option<V5UsageStatus>,
    /// Original response, including fields not represented by this summary.
    ///
    /// This can contain private account and payment information. It is omitted
    /// from `Debug` output, but included when explicitly serializing this value.
    #[serde(default)]
    pub raw_response: Value,
}

impl fmt::Debug for SubscriptionInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionInfo")
            .field("anlas_balance", &self.anlas_balance)
            .field("subscription_active", &self.subscription_active)
            .field("tier", &self.tier)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("v5_usage", &self.v5_usage)
            .finish_non_exhaustive()
    }
}

impl SubscriptionInfo {
    /// Whether this account belongs to the Opus tier.
    #[must_use]
    pub const fn is_opus(&self) -> bool { self.tier >= 3 }

    /// Stable name of the subscription tier, when known.
    #[must_use]
    pub const fn tier_name(&self) -> &'static str {
        match self.tier {
            0 => "paper",
            1 => "tablet",
            2 => "scroll",
            3 => "opus",
            _ => "unknown",
        }
    }
}

impl From<&SubscriptionInfo> for crate::AnlasPricingContext {
    fn from(info: &SubscriptionInfo) -> Self {
        Self {
            tier: info.tier,
            subscription_active: info.subscription_active,
            v5_usage_is_negative: info.v5_usage.is_some_and(|usage| usage.is_negative),
            free_first_image_used: false,
        }
    }
}

#[cfg(test)]
mod tests;
