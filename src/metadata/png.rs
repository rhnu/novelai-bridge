use std::collections::BTreeMap;

use super::PngMetadataWarning;
use crate::{
    BridgeError, MetadataError, RequestField,
    media_policy::{ImageDecodePolicy, ImageLimit},
};

pub struct ExtractedPngText {
    pub(super) text_chunks: BTreeMap<String, Vec<String>>,
    pub(super) warnings: Vec<PngMetadataWarning>,
    #[cfg(feature = "image-processing")]
    pub usage: crate::media_policy::ImageResourceUsage,
    #[cfg(feature = "image-processing")]
    pub pixels: Option<PngPixels>,
}

#[cfg(feature = "image-processing")]
pub struct PngPixels {
    pub bytes: Vec<u8>,
    pub color: png::ColorType,
    pub depth: png::BitDepth,
}

/// Validates the complete PNG once; text interpretation failures remain recoverable.
pub fn extract_png_text_chunks(
    bytes: &[u8],
    policy: ImageDecodePolicy,
    retain_pixels: bool,
) -> Result<ExtractedPngText, BridgeError> {
    policy
        .check(ImageLimit::EncodedBytes, bytes.len() as u64)
        .map_err(invalid_png_payload)?;
    preflight_chunks(bytes, policy)?;
    let mut options = png::DecodeOptions::default();
    options.set_skip_ancillary_crc_failures(false);
    options.set_ignore_adler32(false);
    let mut decoder = png::Decoder::new_with_options(std::io::Cursor::new(bytes), options);
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(invalid_png_payload)?;
    let (width, height) = reader.info().size();
    policy
        .check_dimensions(width, height)
        .map_err(invalid_png_payload)?;
    let output_size = reader
        .output_buffer_size()
        .ok_or(png::DecodingError::LimitsExceeded)
        .map_err(invalid_png_payload)?;
    policy
        .check(ImageLimit::DecodedBytes, output_size as u64)
        .map_err(invalid_png_payload)?;
    #[cfg(feature = "image-processing")]
    let pixels = if retain_pixels {
        let mut output = vec![0_u8; output_size];
        let frame = reader
            .next_frame(&mut output)
            .map_err(invalid_png_payload)?;
        output.truncate(frame.buffer_size());
        Some(PngPixels {
            bytes: output,
            color: frame.color_type,
            depth: frame.bit_depth,
        })
    } else {
        while reader.next_row().map_err(invalid_png_payload)?.is_some() {}
        None
    };
    #[cfg(not(feature = "image-processing"))]
    {
        let _ = retain_pixels;
        while reader.next_row().map_err(invalid_png_payload)?.is_some() {}
    }
    reader.finish().map_err(invalid_png_payload)?;
    let info = reader.info();

    let mut chunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut total_text_bytes = 0_usize;
    let mut max_text_bytes = 0_usize;
    for chunk in &info.uncompressed_latin1_text {
        account_text(
            chunk.text.len(),
            &mut total_text_bytes,
            &mut max_text_bytes,
            policy,
        )?;
        push_text_chunk(&mut chunks, chunk.keyword.as_str(), chunk.text.as_str());
    }
    for chunk in &info.compressed_latin1_text {
        decode_compressed_text_chunk(
            &mut chunks,
            &mut warnings,
            chunk.keyword.as_str(),
            chunk.clone(),
            &mut total_text_bytes,
            &mut max_text_bytes,
            policy,
        );
    }
    for chunk in &info.utf8_text {
        decode_compressed_text_chunk(
            &mut chunks,
            &mut warnings,
            chunk.keyword.as_str(),
            chunk.clone(),
            &mut total_text_bytes,
            &mut max_text_bytes,
            policy,
        );
    }
    Ok(ExtractedPngText {
        text_chunks: chunks,
        warnings,
        #[cfg(feature = "image-processing")]
        usage: crate::media_policy::ImageResourceUsage {
            encoded_bytes: bytes.len() as u64,
            width,
            height,
            decoded_bytes: output_size as u64,
            text_chunk_bytes: max_text_bytes as u64,
            total_text_bytes: total_text_bytes as u64,
        },
        #[cfg(feature = "image-processing")]
        pixels,
    })
}

fn decode_compressed_text_chunk<T>(
    chunks: &mut BTreeMap<String, Vec<String>>,
    warnings: &mut Vec<PngMetadataWarning>,
    keyword: &str,
    mut chunk: T,
    total: &mut usize,
    maximum: &mut usize,
    policy: ImageDecodePolicy,
) where
    T: PngTextChunk,
{
    let remaining = policy.max_total_text_bytes.saturating_sub(*total);
    let limit = policy.max_text_chunk_bytes.min(remaining);
    match chunk.decode_text(limit) {
        Ok(text) => match account_text(text.len(), total, maximum, policy) {
            Ok(()) => push_text_chunk(chunks, keyword, text.as_str()),
            Err(error) => warnings.push(PngMetadataWarning::InvalidTextChunk {
                keyword: keyword.to_owned(),
                message: super::format_error_chain(&error),
            }),
        },
        Err(error) => warnings.push(PngMetadataWarning::InvalidTextChunk {
            keyword: keyword.to_owned(),
            message: error.to_string(),
        }),
    }
}

fn account_text(
    bytes: usize,
    total: &mut usize,
    maximum: &mut usize,
    policy: ImageDecodePolicy,
) -> Result<(), BridgeError> {
    policy
        .check(ImageLimit::TextChunkBytes, bytes as u64)
        .map_err(invalid_png_payload)?;
    let next_total = total.saturating_add(bytes);
    policy
        .check(ImageLimit::TotalTextBytes, next_total as u64)
        .map_err(invalid_png_payload)?;
    *total = next_total;
    *maximum = (*maximum).max(bytes);
    Ok(())
}

// Inspect chunk lengths and the IHDR before the PNG decoder allocates ancillary data or rows.
fn preflight_chunks(bytes: &[u8], policy: ImageDecodePolicy) -> Result<(), BridgeError> {
    if !bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        return Err(invalid_png_payload(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid PNG signature",
        ))
        .into());
    }
    let mut position = 8_usize;
    let mut total = 0_usize;
    let mut maximum = 0_usize;
    while position < bytes.len() {
        let header = bytes
            .get(position..position.saturating_add(8))
            .ok_or_else(|| {
                invalid_png_payload(png::DecodingError::IoError(std::io::Error::from(
                    std::io::ErrorKind::UnexpectedEof,
                )))
            })?;
        let length =
            u32::from_be_bytes(header[..4].try_into().expect("four-byte chunk length")) as usize;
        let end = position
            .checked_add(12)
            .and_then(|value| value.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| {
                invalid_png_payload(std::io::Error::from(std::io::ErrorKind::UnexpectedEof))
            })?;
        let data = &bytes[position + 8..end - 4];
        match &header[4..] {
            b"IHDR" if data.len() == 13 => {
                let width = u32::from_be_bytes(data[..4].try_into().expect("IHDR width"));
                let height = u32::from_be_bytes(data[4..8].try_into().expect("IHDR height"));
                policy
                    .check_dimensions(width, height)
                    .map_err(invalid_png_payload)?;
            },
            b"tEXt" => {
                let text = data
                    .iter()
                    .position(|byte| *byte == 0)
                    .map_or(data, |index| &data[index + 1..]);
                let utf8_bytes = text
                    .iter()
                    .map(|byte| if *byte > 127 { 2 } else { 1 })
                    .sum();
                account_text(utf8_bytes, &mut total, &mut maximum, policy)?;
            },
            b"iTXt" => {
                if let Some(text) = uncompressed_itxt_body(data) {
                    account_text(text.len(), &mut total, &mut maximum, policy)?;
                }
            },
            b"IEND" => break,
            _ => {},
        }
        position = end;
    }
    Ok(())
}

// iTXt stores keyword, two compression bytes, language and translated keyword before text.
// The PNG decoder validates malformed headers; preflight only measures a recognized text body.
fn uncompressed_itxt_body(data: &[u8]) -> Option<&[u8]> {
    let keyword_end = data.iter().position(|byte| *byte == 0)?;
    if data.get(keyword_end + 1) != Some(&0) {
        return None;
    }
    let mut fields = data.get(keyword_end + 3..)?.splitn(3, |byte| *byte == 0);
    fields.next()?;
    fields.next()?;
    fields.next()
}

trait PngTextChunk {
    fn decode_text(&mut self, limit: usize) -> Result<String, png::DecodingError>;
}

impl PngTextChunk for png::text_metadata::ZTXtChunk {
    fn decode_text(&mut self, limit: usize) -> Result<String, png::DecodingError> {
        // The raw inflater is bounded first. Latin-1 conversion may temporarily require twice
        // this budget; account_text checks the actual UTF-8 length before retaining the chunk.
        self.decompress_text_with_limit(limit)?;
        self.get_text()
    }
}

impl PngTextChunk for png::text_metadata::ITXtChunk {
    fn decode_text(&mut self, limit: usize) -> Result<String, png::DecodingError> {
        self.decompress_text_with_limit(limit)?;
        self.get_text()
    }
}

fn invalid_png_payload(source: impl std::error::Error + Send + Sync + 'static) -> MetadataError {
    MetadataError::InvalidPngPayload {
        field: RequestField::MetadataImage,
        source: Box::new(source),
    }
}

fn push_text_chunk(chunks: &mut BTreeMap<String, Vec<String>>, key: &str, value: &str) {
    let key = key.trim();
    if key.is_empty() {
        return;
    }
    chunks
        .entry(key.to_owned())
        .or_default()
        .push(value.to_owned());
}

pub(super) fn first_chunk_value_ci<'a>(
    chunks: &'a BTreeMap<String, Vec<String>>,
    key: &str,
) -> Option<&'a str> {
    chunks
        .iter()
        .find(|(chunk_key, _)| chunk_key.eq_ignore_ascii_case(key))
        .and_then(|(_, values)| values.first())
        .map(String::as_str)
}

pub(super) fn chunk_values_ci<'a>(
    chunks: &'a BTreeMap<String, Vec<String>>,
    key: &str,
) -> impl Iterator<Item = &'a str> {
    chunks
        .iter()
        .filter(move |(chunk_key, _)| chunk_key.eq_ignore_ascii_case(key))
        .flat_map(|(_, values)| values.iter().map(String::as_str))
}
