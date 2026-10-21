#![allow(dead_code, reason = "Each test target selects the shared fixtures it needs.")]

#[cfg(all(test, feature = "image-processing"))]
mod images;
#[cfg(all(test, feature = "client"))]
mod transport;

#[cfg(feature = "image-processing")]
pub use images::*;
#[cfg(feature = "client")]
pub use transport::MockTransport;
