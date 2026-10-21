use super::{
    BUNDLE_IDENTIFIER, FILE_IDENTIFIER, VibeDocument, VibeDocumentError, VibeDocumentLimits,
    VibeDocumentResource, VibeFile, VibeFileKind, hash_string,
};

pub(super) fn validate(
    document: &VibeDocument,
    limits: VibeDocumentLimits,
) -> Result<(), VibeDocumentError> {
    if let VibeDocument::Bundle(bundle) = document {
        if bundle.identifier != BUNDLE_IDENTIFIER {
            return Err(VibeDocumentError::UnsupportedIdentifier { entry: None });
        }
        if bundle.version != 1 {
            return Err(VibeDocumentError::UnsupportedVersion {
                entry: None,
                version: bundle.version,
            });
        }
    }
    validate_files(document.files(), limits)
}

pub(super) fn validate_files(
    files: &[VibeFile],
    limits: VibeDocumentLimits,
) -> Result<(), VibeDocumentError> {
    limits.check(VibeDocumentResource::Vibes, files.len())?;
    let mut budget = PayloadBudget {
        limits,
        cache_entries: 0,
        payload_bytes: 0,
    };
    for (entry, file) in files.iter().enumerate() {
        validate_file(file, entry, &mut budget)?;
    }
    Ok(())
}

fn validate_file(
    file: &VibeFile,
    entry: usize,
    budget: &mut PayloadBudget,
) -> Result<(), VibeDocumentError> {
    if file.identifier != FILE_IDENTIFIER {
        return Err(VibeDocumentError::UnsupportedIdentifier { entry: Some(entry) });
    }
    if file.version != 1 {
        return Err(VibeDocumentError::UnsupportedVersion {
            entry: Some(entry),
            version: file.version,
        });
    }
    if file.id.len() != 64
        || !file
            .id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(VibeDocumentError::InvalidId { entry });
    }
    for payload in [
        file.image.as_deref(),
        file.thumbnail.as_deref(),
        file.import_info
            .as_ref()
            .and_then(|info| info.mask.as_deref()),
    ]
    .into_iter()
    .flatten()
    {
        budget.payload(payload)?;
    }
    if let Some(thumbnail) = file.thumbnail.as_deref().filter(|value| !value.is_empty()) {
        let valid = thumbnail
            .strip_prefix("data:image/")
            .and_then(|value| value.split_once(";base64,"))
            .is_some_and(|(format, _)| !format.is_empty() && !format.contains(';'));
        if !valid {
            return Err(VibeDocumentError::InvalidThumbnail { entry });
        }
    }
    if let Some(info) = &file.import_info {
        finite(
            info.information_extracted,
            entry,
            "importInfo.information_extracted",
        )?;
        finite(info.strength, entry, "importInfo.strength")?;
    }
    for records in file.encodings.values() {
        budget.cache_entry()?;
        for record in records.values() {
            budget.cache_entry()?;
            budget.payload(&record.encoding)?;
            if let Some(params) = &record.params {
                finite(
                    params.information_extracted,
                    entry,
                    "encoding.params.information_extracted",
                )?;
                if let Some(mask) = &params.mask {
                    budget.payload(mask)?;
                }
            }
        }
    }
    let identity_payload = match file.kind {
        VibeFileKind::Image => file
            .image
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or(VibeDocumentError::MissingImage { entry })?,
        VibeFileKind::Encoding => {
            &file
                .encodings
                .values()
                .flat_map(indexmap::IndexMap::values)
                .next()
                .ok_or(VibeDocumentError::MissingEncoding { entry })?
                .encoding
        },
    };
    if hash_string(identity_payload) != file.id {
        return Err(VibeDocumentError::HashMismatch { entry });
    }
    Ok(())
}

fn finite(value: Option<f64>, entry: usize, field: &'static str) -> Result<(), VibeDocumentError> {
    if value.is_some_and(|value| !value.is_finite()) {
        Err(VibeDocumentError::NonFiniteNumber { entry, field })
    } else {
        Ok(())
    }
}

struct PayloadBudget {
    limits: VibeDocumentLimits,
    cache_entries: usize,
    payload_bytes: usize,
}

impl PayloadBudget {
    const fn cache_entry(&mut self) -> Result<(), VibeDocumentError> {
        self.cache_entries = self.cache_entries.saturating_add(1);
        self.limits
            .check(VibeDocumentResource::CacheEntries, self.cache_entries)
    }

    fn payload(&mut self, payload: &str) -> Result<(), VibeDocumentError> {
        self.limits
            .check(VibeDocumentResource::PayloadBytes, payload.len())?;
        self.payload_bytes = self.payload_bytes.saturating_add(payload.len());
        self.limits
            .check(VibeDocumentResource::TotalPayloadBytes, self.payload_bytes)
    }
}
