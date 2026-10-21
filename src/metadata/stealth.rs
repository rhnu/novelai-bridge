use std::io::Read;

use super::{MetadataOrigin, MetadataWarning};
use crate::{ImageDecodePolicy, ImageSize};

const SIGNATURE: &[u8] = b"stealth_pngcomp";
pub(super) const HEADER_BITS: usize = (SIGNATURE.len() + 4) * 8;

fn invalid(message: impl Into<String>) -> MetadataWarning {
    MetadataWarning::InvalidCarrier {
        origin: MetadataOrigin::StealthPngComp,
        message: message.into(),
    }
}

pub(super) fn read_pixels(
    pixels: &image::DynamicImage,
    policy: ImageDecodePolicy,
) -> Result<Option<String>, MetadataWarning> {
    use image::GenericImageView as _;
    if !matches!(
        pixels.color(),
        image::ColorType::Rgba8 | image::ColorType::La8
    ) {
        return Ok(None);
    }
    let size = ImageSize {
        width: pixels.width(),
        height: pixels.height(),
    };
    read_alpha(size, policy, |x, y| {
        pixels.get_pixel(
            u32::try_from(x).expect("validated width"),
            u32::try_from(y).expect("validated height"),
        )[3]
    })
}

pub(super) fn read_alpha(
    size: ImageSize,
    policy: ImageDecodePolicy,
    alpha: impl Fn(usize, usize) -> u8,
) -> Result<Option<String>, MetadataWarning> {
    let pixels = usize::try_from(size.pixels()).map_err(|error| invalid(error.to_string()))?;
    let header_bits = HEADER_BITS;
    if size.height == 0 || pixels < header_bits {
        return Ok(None);
    }
    let height = size.height as usize;
    let mut bit = 0;
    let mut next_byte = || {
        let mut value = 0;
        for _ in 0..8 {
            // Provider order is column-major: x is outer, y is inner; bits are most significant first.
            value = (value << 1) | (alpha(bit / height, bit % height) & 1);
            bit += 1;
        }
        value
    };
    for expected in SIGNATURE {
        if next_byte() != *expected {
            return Ok(None);
        }
    }
    let length = u32::from_be_bytes(std::array::from_fn(|_| next_byte()));
    let length = usize::try_from(length).map_err(|error| invalid(error.to_string()))?;
    if length == 0 || !length.is_multiple_of(8) || length > pixels - header_bits {
        return Err(invalid(
            "stealth payload length is invalid or exceeds alpha capacity",
        ));
    }
    let limit = policy.max_text_chunk_bytes.min(policy.max_total_text_bytes);
    if length / 8 > limit {
        return Err(invalid(
            "compressed stealth payload exceeds the metadata byte budget",
        ));
    }
    let mut compressed = Vec::new();
    for _ in 0..length / 8 {
        compressed.push(next_byte());
    }
    let inflated = decompress(&compressed, limit)?;
    String::from_utf8(inflated)
        .map(Some)
        .map_err(|error| invalid(error.to_string()))
}

fn decompress(compressed: &[u8], limit: usize) -> Result<Vec<u8>, MetadataWarning> {
    if compressed.starts_with(&[0x1F, 0x8B, 0x08]) {
        let mut decoder = flate2::bufread::GzDecoder::new(compressed);
        let text = bounded_read(&mut decoder, limit)?;
        if !decoder.into_inner().is_empty() {
            return Err(invalid("trailing bytes after stealth gzip payload"));
        }
        Ok(text)
    } else if compressed.len() >= 2
        && compressed[0] & 0x0F == 8
        && compressed[0] >> 4 <= 7
        && u16::from_be_bytes([compressed[0], compressed[1]]).is_multiple_of(31)
    {
        inflate(compressed, limit, true)
    } else {
        inflate(compressed, limit, false)
    }
}

fn inflate(compressed: &[u8], limit: usize, zlib: bool) -> Result<Vec<u8>, MetadataWarning> {
    let mut decoder = flate2::Decompress::new(zlib);
    let mut output = Vec::new();
    let mut consumed = 0;
    let mut buffer = [0; 8192];
    loop {
        let remaining = limit.saturating_sub(output.len());
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(
                &compressed[consumed..],
                &mut buffer[..remaining.saturating_add(1).min(8192)],
                flate2::FlushDecompress::None,
            )
            .map_err(|error| invalid(error.to_string()))?;
        let input = usize::try_from(decoder.total_in() - before_in)
            .map_err(|error| invalid(error.to_string()))?;
        let count = usize::try_from(decoder.total_out() - before_out)
            .map_err(|error| invalid(error.to_string()))?;
        consumed += input;
        if count > remaining {
            return Err(invalid(
                "inflated stealth payload exceeds the metadata byte budget",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
        if status == flate2::Status::StreamEnd {
            if consumed != compressed.len() {
                return Err(invalid("trailing bytes after stealth deflate payload"));
            }
            return Ok(output);
        }
        if input == 0 && count == 0 {
            return Err(invalid(if consumed == compressed.len() {
                "truncated stealth deflate payload"
            } else {
                "stealth deflate decoder made no progress"
            }));
        }
    }
}

fn bounded_read(reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, MetadataWarning> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let remaining = limit.saturating_sub(output.len());
        let count = reader
            .read(&mut buffer[..remaining.saturating_add(1).min(8192)])
            .map_err(|error| invalid(error.to_string()))?;
        if count == 0 {
            return Ok(output);
        }
        if count > remaining {
            return Err(invalid(
                "inflated stealth payload exceeds the metadata byte budget",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

#[cfg(test)]
mod tests;
