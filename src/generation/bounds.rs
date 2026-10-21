use crate::InvalidRequest;

pub(crate) fn normalize_float(
    field: &str, value: f32, min: f32, max: f32, reject: bool,
) -> Result<f32, InvalidRequest> {
    if !value.is_finite() {
        return Err(InvalidRequest::NonFiniteNumber { field: field.to_owned(), value: f64::from(value) });
    }
    if reject && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange { field: field.to_owned(), value: f64::from(value), min: f64::from(min), max: f64::from(max) });
    }
    Ok(value.clamp(min, max))
}

pub(crate) fn normalize_integer(
    field: &str, value: u32, min: u32, max: u32, reject: bool,
) -> Result<u32, InvalidRequest> {
    if reject && !(min..=max).contains(&value) {
        return Err(InvalidRequest::NumericOutOfRange { field: field.to_owned(), value: f64::from(value), min: f64::from(min), max: f64::from(max) });
    }
    Ok(value.clamp(min, max))
}
