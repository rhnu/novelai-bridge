//! Validated media inputs, pixel preparation and generated image responses.
mod codec;
mod fit;
mod input;
mod output;
mod preparation;
mod response;
pub use input::EncodedImage;
pub use output::{GeneratedImage, ImageMediaFormat};
#[cfg(any(feature = "generation", test))]
pub use preparation::prepare_image;
pub use preparation::{
    DefaultImagePreprocessor, ImagePreparation, ImagePreprocessor, PreparedImage,
};
#[cfg(feature = "client")]
pub use response::decode_first_generated_image;
#[cfg(feature = "streaming")]
pub use response::validated_generated_image;
pub use response::{decode_generated_images, decode_generated_images_with_policy};

#[cfg(test)]
mod tests;
