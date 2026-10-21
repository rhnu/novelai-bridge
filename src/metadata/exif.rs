use super::{MetadataOrigin, MetadataWarning};
use crate::ImageDecodePolicy;

fn invalid(message: impl Into<String>) -> MetadataWarning {
    MetadataWarning::InvalidCarrier {
        origin: MetadataOrigin::ExifUserComment,
        message: message.into(),
    }
}

/// Locates the bounded Exif segment before the Exif parser allocates its TIFF structure.
pub(super) fn read(
    bytes: &[u8],
    policy: ImageDecodePolicy,
) -> Result<Option<String>, MetadataWarning> {
    let Some(raw) = locate(bytes)? else {
        return Ok(None);
    };
    let raw = raw.strip_prefix(b"Exif\0\0").unwrap_or(raw);
    let limit = policy.max_text_chunk_bytes.min(policy.max_total_text_bytes);
    if raw.len() > limit {
        return Err(invalid("Exif carrier exceeds the metadata byte budget"));
    }
    let parsed = exif::Reader::new()
        .read_raw(raw.to_vec())
        .map_err(|error| invalid(error.to_string()))?;
    let Some(comment) = parsed.get_field(exif::Tag::UserComment, exif::In::PRIMARY) else {
        return Ok(None);
    };
    let text = match &comment.value {
        exif::Value::Undefined(data, _) | exif::Value::Byte(data) => decode_comment(data, limit)?,
        _ => return Err(invalid("Exif UserComment must contain encoded byte data")),
    };
    Ok((!text.is_empty()).then_some(text))
}

fn decode_comment(bytes: &[u8], limit: usize) -> Result<String, MetadataWarning> {
    if bytes.len() > limit {
        return Err(invalid("Exif UserComment exceeds the metadata byte budget"));
    }
    let text = if let Some(mut bytes) = bytes.strip_prefix(b"UNICODE\0") {
        let little_endian = bytes.starts_with(&[0xFF, 0xFE]);
        if little_endian || bytes.starts_with(&[0xFE, 0xFF]) {
            bytes = &bytes[2..];
        }
        if !bytes.len().is_multiple_of(2) {
            return Err(invalid("Exif Unicode comment has an incomplete code unit"));
        }
        let mut text = String::new();
        let units = bytes.as_chunks::<2>().0.iter().map(|pair| {
            if little_endian {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        });
        for value in char::decode_utf16(units) {
            let value = value.map_err(|error| invalid(error.to_string()))?;
            if text.len().saturating_add(value.len_utf8()) > limit {
                return Err(invalid(
                    "decoded Exif comment exceeds the metadata byte budget",
                ));
            }
            text.push(value);
        }
        text
    } else {
        let bytes = bytes
            .strip_prefix(b"ASCII\0\0\0")
            .or_else(|| bytes.strip_prefix(&[0; 8]))
            .unwrap_or(bytes);
        String::from_utf8(bytes.to_vec()).map_err(|error| invalid(error.to_string()))?
    };
    Ok(text.trim_end_matches('\0').to_owned())
}

fn locate(bytes: &[u8]) -> Result<Option<&[u8]>, MetadataWarning> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut rest = &bytes[8..];
        while !rest.is_empty() {
            let header = rest
                .get(..8)
                .ok_or_else(|| invalid("truncated PNG carrier header"))?;
            let length = usize::try_from(u32::from_be_bytes(
                header[..4].try_into().expect("four-byte length"),
            ))
            .map_err(|error| invalid(error.to_string()))?;
            let data = rest
                .get(8..8_usize.saturating_add(length))
                .ok_or_else(|| invalid("truncated PNG carrier"))?;
            if &header[4..] == b"eXIf" {
                return Ok(Some(data));
            }
            if &header[4..] == b"IEND" {
                return Ok(None);
            }
            rest = rest
                .get(length.saturating_add(12)..)
                .ok_or_else(|| invalid("truncated PNG carrier CRC"))?;
        }
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let declared = usize::try_from(u32::from_le_bytes(
            bytes[4..8].try_into().expect("RIFF length"),
        ))
        .map_err(|error| invalid(error.to_string()))?;
        let mut rest = bytes
            .get(12..declared.saturating_add(8))
            .ok_or_else(|| invalid("truncated WebP carrier"))?;
        while !rest.is_empty() {
            let header = rest
                .get(..8)
                .ok_or_else(|| invalid("truncated WebP carrier header"))?;
            let length = usize::try_from(u32::from_le_bytes(
                header[4..8].try_into().expect("four-byte length"),
            ))
            .map_err(|error| invalid(error.to_string()))?;
            let data = rest
                .get(8..8_usize.saturating_add(length))
                .ok_or_else(|| invalid("truncated WebP carrier data"))?;
            if &header[..4] == b"EXIF" {
                return Ok(Some(data));
            }
            rest = rest
                .get(8_usize.saturating_add(length).saturating_add(length % 2)..)
                .ok_or_else(|| invalid("truncated WebP padding"))?;
        }
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut rest = &bytes[2..];
        while let Some((&0xFF, following)) = rest.split_first() {
            rest = following;
            while rest.first() == Some(&0xFF) {
                rest = &rest[1..];
            }
            let Some((&marker, following)) = rest.split_first() else {
                break;
            };
            rest = following;
            if matches!(marker, 0xDA | 0xD9) {
                return Ok(None);
            }
            if matches!(marker, 0x01 | 0xD0..=0xD8) {
                continue;
            }
            let header = rest
                .get(..2)
                .ok_or_else(|| invalid("truncated JPEG segment header"))?;
            let length = usize::from(u16::from_be_bytes([header[0], header[1]]));
            let data = rest
                .get(2..length)
                .ok_or_else(|| invalid("truncated JPEG segment"))?;
            if marker == 0xE1 && data.starts_with(b"Exif\0\0") {
                return Ok(Some(data));
            }
            rest = rest
                .get(length..)
                .ok_or_else(|| invalid("truncated JPEG segment"))?;
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
