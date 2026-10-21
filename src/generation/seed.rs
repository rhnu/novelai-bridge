use crate::PreparationError;

pub(super) fn resolve_seed(seed: i64) -> Result<i64, PreparationError> {
    resolve_seed_with(seed, getrandom::fill)
}

fn resolve_seed_with(
    seed: i64,
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<i64, PreparationError> {
    if seed != 0 {
        return Ok(seed);
    }
    let mut bytes = [0; 4];
    fill(&mut bytes).map_err(|source| PreparationError::SeedGenerationFailed {
        source: Box::new(source),
    })?;
    Ok(i64::from(u32::from_ne_bytes(bytes).max(1)))
}

#[cfg(test)]
mod tests;
