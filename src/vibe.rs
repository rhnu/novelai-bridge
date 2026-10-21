//! Vibe encoding data and transport-independent encode-vibe preparation.
mod encoding;
pub use encoding::VibeEncoding;

mod document;
pub use document::{
    VibeBundle, VibeDocument, VibeDocumentError, VibeDocumentLimits, VibeDocumentResource,
    VibeEncodingParams, VibeEncodingRecord, VibeFile, VibeFileKind, VibeImportInfo,
};

mod png;
mod prepare;
mod reference;
mod request;
mod wire;
pub use prepare::PreparedVibe;
pub use reference::{VibeReference, VibeReferenceInput, VibeTransferConfig};
pub use request::EncodeVibeRequest;
