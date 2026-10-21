//! Vibe encoding data and transport-independent encode-vibe preparation.
mod encoding;
pub use encoding::VibeEncoding;

mod prepare;
mod reference;
mod request;
mod wire;
pub use prepare::PreparedVibe;
pub use reference::{VibeReference, VibeReferenceInput, VibeTransferConfig};
pub use request::EncodeVibeRequest;
