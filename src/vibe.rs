//! Vibe encoding data and transport-independent encode-vibe preparation.
mod encoding;
pub use encoding::VibeEncoding;

#[cfg(feature = "generation")]
mod prepare;
#[cfg(feature = "generation")]
mod request;
#[cfg(feature = "generation")]
mod wire;
#[cfg(feature = "generation")]
pub use prepare::PreparedVibe;
#[cfg(feature = "generation")]
pub use request::{EncodeVibeRequest, VibeReference, VibeTransferConfig};
