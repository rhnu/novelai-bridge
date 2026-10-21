use crate::{InvalidRequest, RequestField, ValidationPolicy};

pub fn normalize_float(
    field: RequestField,
    value: f32,
    min: f32,
    max: f32,
    policy: ValidationPolicy,
) -> Result<f32, InvalidRequest> {
    if !value.is_finite() {
        return Err(InvalidRequest::NonFiniteNumber {
            field,
            value: f64::from(value),
        });
    }
    if policy.is_reject() && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange {
            field,
            value: f64::from(value),
            min: f64::from(min),
            max: f64::from(max),
        });
    }
    Ok(value.clamp(min, max))
}

pub fn normalize_integer(
    field: RequestField,
    value: u32,
    min: u32,
    max: u32,
    policy: ValidationPolicy,
) -> Result<u32, InvalidRequest> {
    if policy.is_reject() && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange {
            field,
            value: f64::from(value),
            min: f64::from(min),
            max: f64::from(max),
        });
    }
    Ok(value.clamp(min, max))
}
