//! Validated media inputs, pixel preparation and generated image responses.
mod fit;
mod input;
mod output;
mod preparation;
mod response;

pub use input::EncodedImage;
pub use output::GeneratedImage;
pub use preparation::{DefaultImagePreprocessor, ImagePreparation, ImagePreprocessor, PreparedImage};
pub use response::decode_generated_images;
#[cfg(any(feature = "generation", test))]
pub(crate) use preparation::prepare_image;
#[cfg(feature = "client")]
pub(crate) use response::decode_first_generated_image;

#[cfg(test)]
mod tests;
