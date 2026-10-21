//! Vibe encoding data and transport-independent encode-vibe preparation.
mod encoding;
pub use encoding::VibeEncoding;

mod prepare;
mod request;
mod wire;
pub use prepare::PreparedVibe;
pub use request::{EncodeVibeRequest, VibeReference, VibeTransferConfig};
