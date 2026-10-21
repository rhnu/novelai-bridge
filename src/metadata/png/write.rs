use std::collections::BTreeSet;

use png::text_metadata::EncodableTextChunk;

use crate::{BridgeError, ImageDecodePolicy, ImageLimit, MetadataError};

/// A UTF-8 text replacement in a PNG. Other chunks retain their original bytes and order.
#[derive(Clone, PartialEq, Eq)]
pub struct PngTextEntry {
    pub keyword: String,
    pub text: String,
}

impl PngTextEntry {
    #[must_use]
    pub fn new(keyword: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            keyword: keyword.into(),
            text: text.into(),
        }
    }
}

impl std::fmt::Debug for PngTextEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PngTextEntry")
            .field("keyword", &self.keyword)
            .field("text_bytes", &self.text.len())
            .finish()
    }
}

/// Replaces matching PNG text keywords without re-encoding pixels or unrelated chunks.
///
/// # Errors
/// Returns invalid-media, keyword, duplicate-key or resource-budget errors.
pub fn write_png_text_chunks(
    bytes: &[u8],
    entries: &[PngTextEntry],
) -> Result<Vec<u8>, BridgeError> {
    write_png_text_chunks_with_policy(bytes, entries, ImageDecodePolicy::default())
}

/// Writes bounded text replacements into a completely validated PNG.
///
/// Keywords match case-insensitively, consistently with metadata reading. Replacements are
/// uncompressed iTXt chunks. Supplying the same keyword more than once is an error.
///
/// # Errors
/// Returns invalid-media, keyword, duplicate-key or resource-budget errors.
pub fn write_png_text_chunks_with_policy(
    bytes: &[u8],
    entries: &[PngTextEntry],
    policy: ImageDecodePolicy,
) -> Result<Vec<u8>, BridgeError> {
    let original = super::extract_png_text_chunks(bytes, policy, false)?;
    let mut keys = BTreeSet::new();
    let mut inserted_text = 0_u64;
    let mut inserted_encoded = 0_u64;
    for entry in entries {
        validate_keyword(&entry.keyword)?;
        if entry.text.contains('\0') {
            return Err(invalid("PNG text contains a null character"));
        }
        if !keys.insert(entry.keyword.to_ascii_lowercase()) {
            return Err(invalid("duplicate replacement keyword"));
        }
        policy
            .check(ImageLimit::TextChunkBytes, entry.text.len() as u64)
            .map_err(invalid_source)?;
        inserted_text = inserted_text.saturating_add(entry.text.len() as u64);
        inserted_encoded = inserted_encoded
            .saturating_add(entry.keyword.chars().count() as u64)
            .saturating_add(entry.text.len() as u64)
            .saturating_add(17);
    }
    let retained_text: u64 = original
        .text_chunks
        .iter()
        .filter(|(key, _)| !keys.contains(&key.to_ascii_lowercase()))
        .flat_map(|(_, values)| values)
        .map(|value| value.len() as u64)
        .sum();
    policy
        .check(
            ImageLimit::TotalTextBytes,
            retained_text.saturating_add(inserted_text),
        )
        .map_err(invalid_source)?;
    let removed = removed_chunk_bytes(bytes, &keys);
    let output_bytes = (bytes.len() as u64)
        .saturating_sub(removed)
        .saturating_add(inserted_encoded);
    policy
        .check(ImageLimit::EncodedBytes, output_bytes)
        .map_err(invalid_source)?;
    let mut additions = Vec::new();
    for entry in entries {
        png::text_metadata::ITXtChunk::new(&entry.keyword, &entry.text)
            .encode(&mut additions)
            .map_err(invalid_source)?;
        policy
            .check(ImageLimit::EncodedBytes, additions.len() as u64)
            .map_err(invalid_source)?;
    }
    let mut result = Vec::new();
    append(&mut result, &bytes[..8], policy)?;
    let mut position = 8;
    while position < bytes.len() {
        // The complete PNG decoder has validated framing, CRCs and IEND before editing.
        let length = validated_chunk_length(bytes, position);
        let end = position + 12 + length;
        let kind = &bytes[position + 4..position + 8];
        let data = &bytes[position + 8..position + 8 + length];
        if kind == b"IEND" {
            append(&mut result, &additions, policy)?;
        }
        if !replaces(kind, data, &keys) {
            append(&mut result, &bytes[position..end], policy)?;
        }
        position = end;
    }
    Ok(result)
}

fn removed_chunk_bytes(bytes: &[u8], keys: &BTreeSet<String>) -> u64 {
    let mut removed = 0_u64;
    let mut position = 8;
    while position < bytes.len() {
        let length = validated_chunk_length(bytes, position);
        let end = position + length + 12;
        if replaces(
            &bytes[position + 4..position + 8],
            &bytes[position + 8..end - 4],
            keys,
        ) {
            removed = removed.saturating_add((length + 12) as u64);
        }
        position = end;
    }
    removed
}

fn replaces(kind: &[u8], data: &[u8], keys: &BTreeSet<String>) -> bool {
    matches!(kind, b"tEXt" | b"zTXt" | b"iTXt")
        && data.iter().position(|byte| *byte == 0).is_some_and(|end| {
            let keyword: String = data[..end].iter().map(|byte| char::from(*byte)).collect();
            keys.contains(&keyword.to_ascii_lowercase())
        })
}

fn append(
    output: &mut Vec<u8>,
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<(), BridgeError> {
    policy
        .check(
            ImageLimit::EncodedBytes,
            (output.len() as u64).saturating_add(bytes.len() as u64),
        )
        .map_err(invalid_source)?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn validate_keyword(keyword: &str) -> Result<(), BridgeError> {
    let valid = (1..=79).contains(&keyword.chars().count())
        && !keyword.starts_with(' ')
        && !keyword.ends_with(' ')
        && !keyword.contains("  ")
        && keyword
            .chars()
            .all(|ch| matches!(u32::from(ch), 32..=126 | 161..=255));
    if valid {
        Ok(())
    } else {
        Err(invalid("invalid PNG text keyword"))
    }
}

fn invalid(message: &str) -> BridgeError {
    invalid_source(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

fn invalid_source(source: impl std::error::Error + Send + Sync + 'static) -> BridgeError {
    MetadataError::InvalidPngText {
        source: Box::new(source),
    }
    .into()
}

fn validated_chunk_length(bytes: &[u8], position: usize) -> usize {
    // Used only after the complete PNG decoder and chunk preflight have validated this slice.
    u32::from_be_bytes(
        bytes[position..position + 4]
            .try_into()
            .expect("validated chunk length"),
    ) as usize
}

#[cfg(test)]
mod tests;
