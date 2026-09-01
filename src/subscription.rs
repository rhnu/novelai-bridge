use serde::{Deserialize, Serialize};

/// Free V5 generation allowance reported by `/user/subscription`.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct V5UsageStatus {
    /// Whether the allowance is currently overdrawn.
    pub is_negative: bool,
    /// Remaining allowance as an integer percentage in the `[0, 100]` range.
    pub percent: u32,
    /// Seconds required to recharge one percent.
    pub seconds_until_next_percent: u64,
}

/// Subscription summary returned by [`crate::Client::get_subscription`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubscriptionInfo {
    /// Combined fixed and purchased Anlas balance.
    pub anlas_balance: i64,
    /// Whether the account is effectively on Opus tier.
    pub is_opus: bool,
    /// Whether the paid subscription is currently active.
    pub subscription_active: bool,
    /// Numeric tier value.
    pub tier: i32,
    /// Lowercase tier name.
    pub tier_name: String,
    /// Optional expiration timestamp in milliseconds since the Unix epoch.
    pub expires_at_ms: Option<u64>,
    /// Free V5 generation allowance, when the account reports one.
    pub v5_usage: Option<V5UsageStatus>,
}
