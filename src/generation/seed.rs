use crate::InvalidRequest;

pub(super) fn resolve_seed(seed: i64) -> Result<i64, InvalidRequest> {
    if seed == 0 {
        let mut bytes = [0; 4];
        getrandom::fill(&mut bytes).map_err(|source| InvalidRequest::SeedGenerationFailed { source: Box::new(source) })?;
        Ok(i64::from(u32::from_ne_bytes(bytes).max(1)))
    } else {
        Ok(seed)
    }
}
