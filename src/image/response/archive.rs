use std::io::{Cursor, Read};

use super::{ResponseBudget, response_error, response_message, validated_generated_image};
use crate::{
    BridgeError, GeneratedImage, GeneratedImageSource, ImageDecodePolicy, ImageLimit,
    ImageMediaFormat,
};

pub(super) fn decode(
    payload: Vec<u8>,
    policy: ImageDecodePolicy,
) -> Result<Vec<GeneratedImage>, BridgeError> {
    policy
        .check(ImageLimit::TotalImageBytes, payload.len() as u64)
        .map_err(response_error)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(payload)).map_err(response_error)?;
    policy
        .check(ImageLimit::ArchiveEntries, archive.len() as u64)
        .map_err(response_error)?;
    let mut budget = ResponseBudget::new(policy);
    let mut images = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(response_error)?;
        let name = file.name().map_err(response_error)?.into_owned();
        if ImageMediaFormat::from_extension(&name).is_none() {
            continue;
        }
        budget.begin_image()?;
        budget.check_bytes(file.size())?;
        let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.by_ref()
            .take(
                policy
                    .max_encoded_bytes
                    .min(policy.max_total_image_bytes.saturating_sub(budget.bytes))
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .map_err(response_error)?;
        budget.add_bytes(bytes.len() as u64)?;
        images.push(validated_generated_image(
            bytes,
            GeneratedImageSource::Archive { name, index },
            policy,
        )?);
    }
    if images.is_empty() {
        return Err(response_message(
            "image archive contains no supported images",
        ));
    }
    Ok(images)
}
