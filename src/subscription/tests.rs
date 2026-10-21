use std::time::{Duration, UNIX_EPOCH};

use serde_json::{Value, json};

use super::wire::SubscriptionResponse;
use crate::{AnlasPricingContext, DecodeError, DecodeTarget, SubscriptionInfo};

fn decode(value: Value, seconds: u64) -> Result<SubscriptionInfo, DecodeError> {
    let response: SubscriptionResponse = serde_json::from_value(value)
        .map_err(|source| DecodeError::new(DecodeTarget::JsonResponse, Box::new(source)))?;
    response.into_info(UNIX_EPOCH + Duration::from_secs(seconds))
}

#[test]
fn subscription_preserves_balance_and_derives_the_single_account_context() {
    let info = decode(
        json!({
            "tier": 3,
            "active": true,
            "expiresAt": 200,
            "trainingStepsLeft": { "fixedTrainingStepsLeft": 100, "purchasedTrainingSteps": 23 },
            "usage": { "percent": 87.6, "isNegative": true, "timeUntilNextPercent": 142 }
        }),
        50,
    )
    .unwrap();
    assert_eq!(info.anlas_balance, 123);
    assert!(info.is_opus());
    assert_eq!(info.tier_name(), "opus");
    assert_eq!(info.expires_at_ms, Some(200_000));
    let usage = info.v5_usage.unwrap();
    assert_eq!(usage.percent, 87);
    assert_eq!(usage.seconds_until_next_percent, 142);
    let context = AnlasPricingContext::from(&info);
    assert_eq!(context.tier, 3);
    assert!(context.subscription_active);
    assert!(context.v5_usage_is_negative);
}

#[test]
fn missing_recharge_rate_preserves_the_overdrawn_allowance() {
    let info = decode(
        json!({
            "tier": 3,
            "trainingStepsLeft": { "fixedTrainingStepsLeft": 10 },
            "usage": { "percent": 0, "isNegative": true }
        }),
        50,
    )
    .unwrap();
    let usage = info.v5_usage.unwrap();
    assert!(usage.is_negative);
    assert_eq!(usage.seconds_until_next_percent, 0);
}

#[test]
fn activity_uses_the_injected_time_and_honors_explicit_state() {
    for (expires, active, expected) in [
        (100, None, false),
        (101, None, true),
        (101, Some(false), false),
        (99, Some(true), true),
    ] {
        let mut response = json!({
            "tier": 3,
            "expiresAt": expires,
            "trainingStepsLeft": { "fixedTrainingStepsLeft": 0 }
        });
        if let Some(active) = active {
            response["active"] = active.into();
        }
        assert_eq!(decode(response, 100).unwrap().subscription_active, expected);
    }
}

#[test]
fn malformed_or_incomplete_subscription_is_an_error() {
    for value in [
        Value::Null,
        json!({}),
        json!({ "tier": 3 }),
        json!({ "tier": "3", "trainingStepsLeft": { "fixedTrainingStepsLeft": 0 } }),
        json!({ "tier": 3, "trainingStepsLeft": {} }),
        json!({ "tier": -1, "trainingStepsLeft": { "fixedTrainingStepsLeft": 0 } }),
        json!({ "tier": 3, "trainingStepsLeft": { "fixedTrainingStepsLeft": "invalid" } }),
        json!({ "tier": 3, "trainingStepsLeft": { "fixedTrainingStepsLeft": -1 } }),
        json!({ "tier": 3, "trainingStepsLeft": { "fixedTrainingStepsLeft": i64::MAX, "purchasedTrainingSteps": 1 } }),
    ] {
        assert!(decode(value, 100).is_err());
    }
}

#[test]
fn supported_field_aliases_map_to_the_same_account() {
    let info = decode(json!({
        "tier": 1,
        "is_active": true,
        "expires_at": 200,
        "training_steps_left": { "fixed_training_steps_left": 10, "purchased_training_steps": 2 },
        "usage": { "percent": 100, "is_negative": false, "time_until_next_percent": 5 }
    }), 100).unwrap();
    assert_eq!(info.anlas_balance, 12);
    assert!(info.subscription_active);
    assert_eq!(info.v5_usage.unwrap().seconds_until_next_percent, 5);
    assert!(!info.is_opus());
}
