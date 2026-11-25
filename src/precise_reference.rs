//! Bounded local exchange of NovelAI's precise reference image ZIP.
use std::{
    collections::HashSet,
    io::{self, Cursor, Read, Write},
};

use serde::{Deserialize, Serialize};
use zip::{ZipArchive, ZipWriter, result::ZipError, write::SimpleFileOptions};

use crate::{
    BridgeError, CharacterReference, CharacterReferenceType, EncodedImage, ImageDecodePolicy,
    ImageLimit, ImagePolicyError, RequestField,
};

const MANIFEST: &str = "metadata.json";

/// A local NovelAI `reference_images.zip`, in manifest order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreciseReferenceDocument {
    pub entries: Vec<PreciseReferenceEntry>,
}

/// Original image and editable settings from one entry of an official archive.
#[derive(Clone, Debug, PartialEq)]
pub struct PreciseReferenceEntry {
    /// The image basename, before any duplicate-name adjustment on export.
    pub filename: String,
    pub enabled: bool,
    pub reference: CharacterReference,
}

/// A local archive failure, with the underlying parser, I/O or image error retained.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PreciseReferenceDocumentError {
    #[error("invalid precise reference ZIP")]
    Zip(#[from] ZipError),
    #[error("cannot read or write a precise reference ZIP entry")]
    Io(#[from] io::Error),
    #[error("invalid precise reference metadata.json")]
    Json(#[from] serde_json::Error),
    #[error("precise reference ZIP has no metadata.json file")]
    MissingManifest,
    #[error("precise reference metadata size ({actual}) exceeds the configured limit ({maximum})")]
    ManifestTooLarge { actual: u64, maximum: u64 },
    #[error("precise reference entry {index} has an invalid image filename")]
    InvalidFilename { index: usize },
    #[error("precise reference entry {index} has no matching image file")]
    MissingImage { index: usize },
    #[error("precise reference entry {index} has invalid image media")]
    Image {
        index: usize,
        #[source]
        source: Box<BridgeError>,
    },
    #[error("precise reference entry {index} has a non-finite parameter")]
    NonFinite { index: usize },
    #[error(transparent)]
    Limit(#[from] ImagePolicyError),
}

#[derive(Deserialize, Serialize)]
struct ManifestEntry {
    filename: String,
    description: CharacterReferenceType,
    information_extracted: f32,
    fidelity: f32,
    strength: f32,
    enabled: Option<bool>,
}

impl ManifestEntry {
    fn check_numbers(&self, index: usize) -> Result<(), PreciseReferenceDocumentError> {
        if [self.information_extracted, self.fidelity, self.strength]
            .into_iter()
            .all(f32::is_finite)
        {
            Ok(())
        } else {
            Err(PreciseReferenceDocumentError::NonFinite { index })
        }
    }
}

impl PreciseReferenceDocument {
    /// Reads images and parameters from an official archive without contacting NovelAI.
    ///
    /// # Errors
    /// Returns malformed ZIP, manifest or image errors, or exceeded media budgets.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PreciseReferenceDocumentError> {
        Self::from_bytes_with_policy(bytes, ImageDecodePolicy::default())
    }

    /// Reads an archive with the SDK media budgets. The text chunk limit caps the
    /// manifest and the decoded-byte limit also caps total retained decoded pixels.
    ///
    /// # Errors
    /// Returns malformed ZIP, manifest or image errors, or exceeded media budgets.
    pub fn from_bytes_with_policy(
        bytes: &[u8],
        policy: ImageDecodePolicy,
    ) -> Result<Self, PreciseReferenceDocumentError> {
        policy.check(ImageLimit::TotalImageBytes, bytes.len() as u64)?;
        let mut archive = ZipArchive::new(Cursor::new(bytes))?;
        policy.check(ImageLimit::ArchiveEntries, archive.len() as u64)?;

        let manifest = {
            let mut file = archive.by_name(MANIFEST).map_err(|error| match error {
                ZipError::FileNotFound => PreciseReferenceDocumentError::MissingManifest,
                error => error.into(),
            })?;
            if !file.is_file() {
                return Err(PreciseReferenceDocumentError::MissingManifest);
            }
            let maximum = policy.max_text_chunk_bytes as u64;
            check_manifest_size(file.size(), maximum)?;
            let bytes = read_member(&mut file, maximum)?;
            check_manifest_size(bytes.len() as u64, maximum)?;
            serde_json::from_slice::<Vec<ManifestEntry>>(&bytes)?
        };
        policy.check(ImageLimit::Images, manifest.len() as u64)?;

        let mut entries = Vec::with_capacity(manifest.len());
        let mut encoded_bytes = 0_u64;
        let mut decoded_bytes = 0_u64;
        for (index, metadata) in manifest.into_iter().enumerate() {
            metadata.check_numbers(index)?;
            if !valid_filename(&metadata.filename) {
                return Err(PreciseReferenceDocumentError::InvalidFilename { index });
            }
            let mut file = archive
                .by_name(&metadata.filename)
                .map_err(|error| match error {
                    ZipError::FileNotFound => PreciseReferenceDocumentError::MissingImage { index },
                    error => error.into(),
                })?;
            if !file.is_file() {
                return Err(PreciseReferenceDocumentError::MissingImage { index });
            }

            policy.check(ImageLimit::EncodedBytes, file.size())?;
            policy.check(
                ImageLimit::TotalImageBytes,
                encoded_bytes.saturating_add(file.size()),
            )?;
            let maximum = policy
                .max_encoded_bytes
                .min(policy.max_total_image_bytes - encoded_bytes);
            let bytes = read_member(&mut file, maximum)?;
            policy.check(ImageLimit::EncodedBytes, bytes.len() as u64)?;
            encoded_bytes = encoded_bytes.saturating_add(bytes.len() as u64);
            policy.check(ImageLimit::TotalImageBytes, encoded_bytes)?;

            let image = EncodedImage::from_bytes_with_policy(
                bytes,
                ImageDecodePolicy {
                    max_decoded_bytes: policy.max_decoded_bytes - decoded_bytes,
                    ..policy
                },
            )
            .map_err(|source| PreciseReferenceDocumentError::Image {
                index,
                source: Box::new(source),
            })?;
            decoded_bytes = decoded_bytes.saturating_add(image.pixels().as_bytes().len() as u64);
            policy.check(ImageLimit::DecodedBytes, decoded_bytes)?;
            entries.push(PreciseReferenceEntry {
                filename: metadata.filename,
                enabled: metadata.enabled.unwrap_or(true),
                reference: CharacterReference {
                    image,
                    reference_type: metadata.description,
                    information_extracted: metadata.information_extracted,
                    fidelity: metadata.fidelity,
                    strength: metadata.strength,
                },
            });
        }
        Ok(Self { entries })
    }

    /// Writes the official archive with the original image bytes and explicit enabled state.
    ///
    /// # Errors
    /// Returns an error for invalid parameters, exceeded media budgets or failed ZIP writes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PreciseReferenceDocumentError> {
        self.to_bytes_with_policy(ImageDecodePolicy::default())
    }

    /// Writes an archive with explicit media budgets.
    ///
    /// # Errors
    /// Returns an error for invalid parameters, exceeded media budgets or failed ZIP writes.
    pub fn to_bytes_with_policy(
        &self,
        policy: ImageDecodePolicy,
    ) -> Result<Vec<u8>, PreciseReferenceDocumentError> {
        policy.check(ImageLimit::Images, self.entries.len() as u64)?;
        policy.check(ImageLimit::ArchiveEntries, self.entries.len() as u64 + 1)?;
        let mut names = HashSet::new();
        let mut manifest = Vec::with_capacity(self.entries.len());
        let mut encoded_bytes = 0_u64;
        let mut decoded_bytes = 0_u64;
        for (index, entry) in self.entries.iter().enumerate() {
            let reference = &entry.reference;
            reference
                .image
                .validate_policy(RequestField::Image, policy)
                .map_err(|source| PreciseReferenceDocumentError::Image {
                    index,
                    source: Box::new(source),
                })?;
            encoded_bytes = encoded_bytes.saturating_add(reference.image.as_bytes().len() as u64);
            policy.check(ImageLimit::TotalImageBytes, encoded_bytes)?;
            decoded_bytes =
                decoded_bytes.saturating_add(reference.image.pixels().as_bytes().len() as u64);
            policy.check(ImageLimit::DecodedBytes, decoded_bytes)?;

            let filename = unique_filename(entry, index, &mut names)?;
            let metadata = ManifestEntry {
                filename,
                description: reference.reference_type,
                information_extracted: reference.information_extracted,
                fidelity: reference.fidelity,
                strength: reference.strength,
                enabled: Some(entry.enabled),
            };
            metadata.check_numbers(index)?;
            manifest.push(metadata);
        }
        let bytes = serde_json::to_vec(&manifest)?;
        check_manifest_size(bytes.len() as u64, policy.max_text_chunk_bytes as u64)?;
        policy.check(
            ImageLimit::TotalImageBytes,
            encoded_bytes.saturating_add(bytes.len() as u64),
        )?;

        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        // Inputs are already compressed media, and storing the entries keeps ZIP overhead predictable.
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (entry, metadata) in self.entries.iter().zip(&manifest) {
            zip.start_file(&metadata.filename, options)?;
            zip.write_all(entry.reference.image.as_bytes())?;
        }
        zip.start_file(MANIFEST, options)?;
        zip.write_all(&bytes)?;
        let bytes = zip.finish()?.into_inner();
        policy.check(ImageLimit::TotalImageBytes, bytes.len() as u64)?;
        Ok(bytes)
    }
}

fn valid_filename(name: &str) -> bool {
    !matches!(name, "" | "." | "..")
        && name.len() <= usize::from(u16::MAX)
        && !name.eq_ignore_ascii_case(MANIFEST)
        && !name.contains(['/', '\\', '\0'])
}

fn unique_filename(
    entry: &PreciseReferenceEntry,
    index: usize,
    names: &mut HashSet<String>,
) -> Result<String, PreciseReferenceDocumentError> {
    let name = entry
        .filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let mut filename = if valid_filename(name) {
        name.to_owned()
    } else {
        let format = image::guess_format(entry.reference.image.as_bytes())
            .map_err(|_| PreciseReferenceDocumentError::InvalidFilename { index })?;
        let extension = format.extensions_str()[0];
        format!("reference_{}.{extension}", index + 1)
    };
    if names.insert(filename.clone()) {
        return Ok(filename);
    }
    let (stem, extension) = filename
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map_or_else(
            || (filename.clone(), String::new()),
            |(stem, extension)| (stem.to_owned(), format!(".{extension}")),
        );
    for suffix in 1..=names.len() {
        filename = format!("{stem}_{suffix}{extension}");
        if valid_filename(&filename) && names.insert(filename.clone()) {
            return Ok(filename);
        }
    }
    Err(PreciseReferenceDocumentError::InvalidFilename { index })
}

const fn check_manifest_size(
    actual: u64,
    maximum: u64,
) -> Result<(), PreciseReferenceDocumentError> {
    if actual > maximum {
        Err(PreciseReferenceDocumentError::ManifestTooLarge { actual, maximum })
    } else {
        Ok(())
    }
}

fn read_member(file: impl Read, maximum: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;
