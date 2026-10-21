use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::{DecodeError, DecodeTarget, SubscriptionInfo, V5UsageStatus};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionResponse {
    tier: i32,
    #[serde(alias = "isActive", alias = "is_active")]
    active: Option<bool>,
    #[serde(alias = "expires_at")]
    expires_at: Option<u64>,
    #[serde(alias = "training_steps_left")]
    training_steps_left: TrainingSteps,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrainingSteps {
    #[serde(alias = "fixed_training_steps_left")]
    fixed_training_steps_left: Option<i64>,
    #[serde(alias = "purchased_training_steps")]
    purchased_training_steps: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    percent: f64,
    #[serde(default)]
    #[serde(alias = "time_until_next_percent")]
    time_until_next_percent: u64,
    #[serde(default)]
    #[serde(alias = "is_negative")]
    is_negative: bool,
}

impl SubscriptionResponse {
    pub fn into_info(self, now: SystemTime) -> Result<SubscriptionInfo, DecodeError> {
        if self.tier < 0 {
            return Err(invalid_response("subscription tier cannot be negative"));
        }
        let training = self.training_steps_left;
        if training.fixed_training_steps_left.is_none()
            && training.purchased_training_steps.is_none()
        {
            return Err(invalid_response(
                "subscription balance has no reported components",
            ));
        }
        let anlas_balance = training
            .fixed_training_steps_left
            .unwrap_or(0)
            .checked_add(training.purchased_training_steps.unwrap_or(0))
            .filter(|balance| *balance >= 0)
            .ok_or_else(|| {
                invalid_response("subscription balance is outside the supported range")
            })?;
        let expires_at_ms = self
            .expires_at
            .map(|seconds| {
                seconds.checked_mul(1000).ok_or_else(|| {
                    invalid_response("subscription expiration is outside the supported range")
                })
            })
            .transpose()?;
        let subscription_active = self.active.unwrap_or_else(|| {
            self.tier > 0
                && self.expires_at.is_none_or(|seconds| {
                    now.duration_since(UNIX_EPOCH)
                        .is_ok_and(|elapsed| seconds > elapsed.as_secs())
                })
        });
        let v5_usage = self.usage.map(Usage::into_status).transpose()?;
        Ok(SubscriptionInfo {
            anlas_balance,
            subscription_active,
            tier: self.tier,
            expires_at_ms,
            v5_usage,
        })
    }
}

impl Usage {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "finite percent is clamped to the u32 range before truncation"
    )]
    fn into_status(self) -> Result<V5UsageStatus, DecodeError> {
        if !self.percent.is_finite() {
            return Err(invalid_response(
                "subscription usage percent must be finite",
            ));
        }
        Ok(V5UsageStatus {
            is_negative: self.is_negative,
            percent: self.percent.clamp(0.0, 100.0).trunc() as u32,
            seconds_until_next_percent: self.time_until_next_percent,
        })
    }
}

fn invalid_response(message: &'static str) -> DecodeError {
    DecodeError::new(
        DecodeTarget::JsonResponse,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}
