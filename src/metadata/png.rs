use std::collections::BTreeMap;

use super::PngMetadataWarning;
use crate::{BridgeError, MetadataError};
const TEXT_CHUNK_DECOMPRESSION_LIMIT: usize = 2 * 1024 * 1024;

pub(super) struct ExtractedPngText {
    pub(super) text_chunks: BTreeMap<String, Vec<String>>,
    pub(super) warnings: Vec<PngMetadataWarning>,
}

pub(super) fn extract_png_text_chunks(bytes: &[u8]) -> Result<ExtractedPngText, BridgeError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(invalid_png_payload)?;
    let output_size = reader
        .output_buffer_size()
        .ok_or(png::DecodingError::LimitsExceeded)
        .map_err(invalid_png_payload)?;
    let mut output = vec![0_u8; output_size];
    reader
        .next_frame(output.as_mut_slice())
        .map_err(invalid_png_payload)?;
    reader.finish().map_err(invalid_png_payload)?;
    let info = reader.info();

    let mut chunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut warnings = Vec::new();
    for chunk in &info.uncompressed_latin1_text {
        push_text_chunk(&mut chunks, chunk.keyword.as_str(), chunk.text.as_str());
    }
    for chunk in &info.compressed_latin1_text {
        decode_compressed_text_chunk(
            &mut chunks,
            &mut warnings,
            chunk.keyword.as_str(),
            chunk.clone(),
        );
    }
    for chunk in &info.utf8_text {
        decode_compressed_text_chunk(
            &mut chunks,
            &mut warnings,
            chunk.keyword.as_str(),
            chunk.clone(),
        );
    }
    Ok(ExtractedPngText {
        text_chunks: chunks,
        warnings,
    })
}

fn decode_compressed_text_chunk<T>(
    chunks: &mut BTreeMap<String, Vec<String>>,
    warnings: &mut Vec<PngMetadataWarning>,
    keyword: &str,
    mut chunk: T,
) where
    T: PngTextChunk,
{
    match chunk.decode_text(TEXT_CHUNK_DECOMPRESSION_LIMIT) {
        Ok(text) => push_text_chunk(chunks, keyword, text.as_str()),
        Err(error) => warnings.push(PngMetadataWarning::InvalidTextChunk {
            keyword: keyword.to_owned(),
            message: error.to_string(),
        }),
    }
}

trait PngTextChunk {
    fn decode_text(&mut self, limit: usize) -> Result<String, png::DecodingError>;
}

impl PngTextChunk for png::text_metadata::ZTXtChunk {
    fn decode_text(&mut self, limit: usize) -> Result<String, png::DecodingError> {
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

fn invalid_png_payload(source: png::DecodingError) -> MetadataError {
    MetadataError::InvalidPngPayload {
        field: "metadata.image".to_owned(),
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
