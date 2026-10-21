//! Official local vibe files; importing or exporting never performs remote encoding.
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::VibeEncoding;

mod limits;
mod preflight;
mod validation;
pub use limits::{VibeDocumentError, VibeDocumentLimits, VibeDocumentResource};

const FILE_IDENTIFIER: &str = "novelai-vibe-transfer";
const BUNDLE_IDENTIFIER: &str = "novelai-vibe-transfer-bundle";

#[derive(Deserialize)]
struct Identity<'a> {
    #[serde(borrow)]
    identifier: std::borrow::Cow<'a, str>,
}

/// One official `.naiv4vibe` file or `.naiv4vibebundle` container.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum VibeDocument {
    Single(Box<VibeFile>),
    Bundle(VibeBundle),
}

/// Payload identity used by an official vibe file.
#[derive(Copy, Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VibeFileKind {
    Image,
    Encoding,
}

/// Original local file fields. Payload strings and both cache-map orders are retained.
///
/// Model strings in this document describe stored data and do not grant generation capabilities.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeFile {
    pub identifier: String,
    pub version: u32,
    #[serde(rename = "type")]
    pub kind: VibeFileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Lowercase SHA-256 of the original image string, or first encoding string.
    pub id: String,
    /// Model key → parameter-cache key → encoding record, in original insertion order.
    #[serde(default)]
    pub encodings: IndexMap<String, IndexMap<String, VibeEncodingRecord>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// Milliseconds since the Unix epoch, without an implicit current-time fallback.
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(
        rename = "importInfo",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub import_info: Option<VibeImportInfo>,
}

/// Official bundle, retaining the input order of its files.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeBundle {
    pub identifier: String,
    pub version: u32,
    pub vibes: Vec<VibeFile>,
}

/// One cache record; the encoding text is kept unchanged until explicitly selected.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct VibeEncodingRecord {
    pub encoding: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<VibeEncodingParams>,
}

/// Parameters associated with an existing cache key; keys are never recomputed implicitly.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VibeEncodingParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub information_extracted: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
}

/// Optional settings recorded by the frontend when exporting a reference.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VibeImportInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub information_extracted: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<String>,
}

impl VibeDocument {
    /// Parses and validates an official local document with default resource budgets.
    ///
    /// # Errors
    /// Returns invalid-format, identity or resource-limit errors. Bundles fail atomically.
    pub fn from_json(bytes: &[u8]) -> Result<Self, VibeDocumentError> {
        Self::from_json_with_limits(bytes, VibeDocumentLimits::default())
    }

    /// Parses a document after checking the total input budget before deserialization.
    ///
    /// # Errors
    /// Returns invalid-format, identity or resource-limit errors.
    pub fn from_json_with_limits(
        bytes: &[u8],
        limits: VibeDocumentLimits,
    ) -> Result<Self, VibeDocumentError> {
        limits.check(VibeDocumentResource::DocumentBytes, bytes.len())?;
        let identity: Identity<'_> = serde_json::from_slice(bytes)?;
        preflight::check(bytes, identity.identifier == BUNDLE_IDENTIFIER, limits)?;
        let document = match identity.identifier.as_ref() {
            FILE_IDENTIFIER => Self::Single(serde_json::from_slice(bytes)?),
            BUNDLE_IDENTIFIER => Self::Bundle(serde_json::from_slice(bytes)?),
            _ => return Err(VibeDocumentError::UnsupportedIdentifier { entry: None }),
        };
        document.validate_with_limits(limits)?;
        Ok(document)
    }

    /// Validates editable document fields using default resource budgets.
    ///
    /// # Errors
    /// Returns invalid-format, identity or resource-limit errors.
    pub fn validate(&self) -> Result<(), VibeDocumentError> {
        self.validate_with_limits(VibeDocumentLimits::default())
    }

    /// Validates editable fields without normalizing strings or generation parameters.
    ///
    /// # Errors
    /// Returns invalid-format, identity or resource-limit errors.
    pub fn validate_with_limits(
        &self,
        limits: VibeDocumentLimits,
    ) -> Result<(), VibeDocumentError> {
        validation::validate(self, limits)
    }

    /// Exports validated JSON using default resource budgets.
    ///
    /// # Errors
    /// Returns invalid-format, identity, serialization or resource-limit errors.
    pub fn to_json(&self) -> Result<Vec<u8>, VibeDocumentError> {
        self.to_json_with_limits(VibeDocumentLimits::default())
    }

    /// Exports pretty JSON, bounding output growth while serializing.
    ///
    /// # Errors
    /// Returns invalid-format, identity, serialization or resource-limit errors.
    pub fn to_json_with_limits(
        &self,
        limits: VibeDocumentLimits,
    ) -> Result<Vec<u8>, VibeDocumentError> {
        self.validate_with_limits(limits)?;
        limits::encode(self, limits)
    }

    /// Files in provider order; a single document returns a one-element slice.
    #[must_use]
    pub fn files(&self) -> &[VibeFile] {
        match self {
            Self::Single(file) => std::slice::from_ref(file.as_ref()),
            Self::Bundle(bundle) => &bundle.vibes,
        }
    }
}

impl VibeFile {
    /// Creates an image document, hashing the exact input string without decoding it.
    #[must_use]
    pub fn image(image: impl Into<String>) -> Self {
        let image = image.into();
        Self {
            identifier: FILE_IDENTIFIER.to_owned(),
            version: 1,
            kind: VibeFileKind::Image,
            id: hash_string(&image),
            image: Some(image),
            encodings: IndexMap::new(),
            name: None,
            thumbnail: None,
            created_at: None,
            import_info: None,
        }
    }

    /// Creates an encoding document with the frontend's `unknown` parameter-cache key.
    #[must_use]
    pub fn encoded(model_key: impl Into<String>, encoding: impl Into<String>) -> Self {
        let encoding = encoding.into();
        let id = hash_string(&encoding);
        Self {
            identifier: FILE_IDENTIFIER.to_owned(),
            version: 1,
            kind: VibeFileKind::Encoding,
            id,
            image: None,
            encodings: IndexMap::from([(
                model_key.into(),
                IndexMap::from([(
                    "unknown".to_owned(),
                    VibeEncodingRecord {
                        encoding,
                        params: None,
                    },
                )]),
            )]),
            name: None,
            thumbnail: None,
            created_at: None,
            import_info: None,
        }
    }

    /// Selects one existing cache record; a missing key returns `None` without encoding anything.
    ///
    /// # Errors
    /// Returns document validation errors or invalid/empty base64 in the selected record.
    pub fn encoding(
        &self,
        model_key: &str,
        parameter_key: &str,
    ) -> Result<Option<VibeEncoding>, VibeDocumentError> {
        self.encoding_with_limits(model_key, parameter_key, VibeDocumentLimits::default())
    }

    /// Selects an existing cache within explicit document budgets.
    ///
    /// # Errors
    /// Returns document validation errors or invalid/empty base64 in the selected record.
    pub fn encoding_with_limits(
        &self,
        model_key: &str,
        parameter_key: &str,
        limits: VibeDocumentLimits,
    ) -> Result<Option<VibeEncoding>, VibeDocumentError> {
        validation::validate_files(std::slice::from_ref(self), limits)?;
        self.encodings
            .get(model_key)
            .and_then(|records| records.get(parameter_key))
            .map(|record| {
                VibeEncoding::from_base64(&record.encoding).map_err(|source| {
                    VibeDocumentError::InvalidEncoding {
                        source: Box::new(source),
                    }
                })
            })
            .transpose()
    }
}

impl VibeBundle {
    /// Creates an ordered bundle without dropping or merging any input file.
    #[must_use]
    pub fn new(vibes: Vec<VibeFile>) -> Self {
        Self {
            identifier: BUNDLE_IDENTIFIER.to_owned(),
            version: 1,
            vibes,
        }
    }
}

fn hash_string(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(value.as_bytes())
        .iter()
        .flat_map(|&byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

impl std::fmt::Debug for VibeFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeFile")
            .field("kind", &self.kind)
            .field("version", &self.version)
            .field("model_groups", &self.encodings.len())
            .field("has_image", &self.image.is_some())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VibeBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeBundle")
            .field("version", &self.version)
            .field("files", &self.vibes.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VibeEncodingRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeEncodingRecord")
            .field("encoding_bytes", &self.encoding.len())
            .field("has_params", &self.params.is_some())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VibeEncodingParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeEncodingParams")
            .field("information_extracted", &self.information_extracted)
            .field("has_mask", &self.mask.is_some())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for VibeImportInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VibeImportInfo")
            .field("has_model", &self.model.is_some())
            .field("has_mask", &self.mask.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
