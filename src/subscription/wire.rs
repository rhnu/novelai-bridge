use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;

use crate::{DecodeError, DecodeTarget, SubscriptionInfo, V5UsageStatus};

#[derive(Deserialize)]
#[serde(try_from = "Value")]
pub struct SubscriptionResponse {
    fields: SubscriptionFields,
    raw_response: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionFields {
    tier: i32,
    #[serde(alias = "isActive", alias = "is_active")]
    active: Option<bool>,
    #[serde(alias = "expires_at")]
    expires_at: Option<i64>,
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
    percent: u32,
    #[serde(default, alias = "purchased_percent")]
    purchased_percent: u32,
    #[serde(default)]
    #[serde(alias = "time_until_next_percent")]
    time_until_next_percent: u64,
    #[serde(default)]
    #[serde(alias = "is_negative")]
    is_negative: bool,
}

impl TryFrom<Value> for SubscriptionResponse {
    type Error = serde_json::Error;

    fn try_from(raw_response: Value) -> Result<Self, Self::Error> {
        let fields = serde_json::from_value(raw_response.clone())?;
        Ok(Self {
            fields,
            raw_response,
        })
    }
}

impl SubscriptionResponse {
    pub fn into_info(self, now: SystemTime) -> Result<SubscriptionInfo, DecodeError> {
        let Self {
            fields,
            raw_response,
        } = self;
        if fields.tier < 0 {
            return Err(invalid_response("subscription tier cannot be negative"));
        }
        let training = fields.training_steps_left;
        if training.fixed_training_steps_left.is_none()
            && training.purchased_training_steps.is_none()
        {
            return Err(invalid_response(
                "subscription balance has no reported components",
            ));
        }
        if training
            .fixed_training_steps_left
            .is_some_and(|value| value < 0)
            || training
                .purchased_training_steps
                .is_some_and(|value| value < 0)
        {
            return Err(invalid_response(
                "subscription balance components cannot be negative",
            ));
        }
        let anlas_balance = training
            .fixed_training_steps_left
            .unwrap_or(0)
            .checked_add(training.purchased_training_steps.unwrap_or(0))
            .ok_or_else(|| {
                invalid_response("subscription balance is outside the supported range")
            })?;
        let expires_at_ms = fields
            .expires_at
            .map(|seconds| {
                seconds.checked_mul(1000).ok_or_else(|| {
                    invalid_response("subscription expiration is outside the supported range")
                })
            })
            .transpose()?;
        let subscription_active = fields.active.unwrap_or_else(|| {
            fields.tier > 0
                && fields.expires_at.is_none_or(|seconds| {
                    now.duration_since(UNIX_EPOCH)
                        .is_ok_and(|elapsed| i128::from(seconds) > i128::from(elapsed.as_secs()))
                })
        });
        let v5_usage = fields.usage.map(Usage::into_status);
        Ok(SubscriptionInfo {
            anlas_balance,
            subscription_active,
            tier: fields.tier,
            expires_at_ms,
            v5_usage,
            raw_response,
        })
    }
}

impl Usage {
    const fn into_status(self) -> V5UsageStatus {
        V5UsageStatus {
            is_negative: self.is_negative,
            percent: self.percent,
            purchased_percent: self.purchased_percent,
            seconds_until_next_percent: self.time_until_next_percent,
        }
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
