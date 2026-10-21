//! Validated media inputs, pixel preparation and generated image responses.
mod fit;
mod input;
mod output;
mod preparation;
mod response;

pub use input::EncodedImage;
pub use output::GeneratedImage;
#[cfg(any(feature = "generation", test))]
pub use preparation::prepare_image;
pub use preparation::{
    DefaultImagePreprocessor, ImagePreparation, ImagePreprocessor, PreparedImage,
};
#[cfg(feature = "client")]
pub use response::decode_first_generated_image;
pub use response::decode_generated_images;

#[cfg(test)]
mod tests;
