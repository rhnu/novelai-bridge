//! Validated media inputs, pixel preparation and generated image responses.
mod codec;
mod fit;
mod input;
mod output;
mod preparation;
mod response;
pub use input::EncodedImage;
pub use output::{GeneratedImage, GeneratedImageSource, ImageMediaFormat};
pub use preparation::{
    DefaultImagePreprocessor, ImagePreparation, ImagePreprocessor, PreparedImage, prepare_image,
};
pub use response::{
    decode_generated_images, decode_generated_images_with_policy, validated_generated_image,
};

#[cfg(test)]
mod tests;
