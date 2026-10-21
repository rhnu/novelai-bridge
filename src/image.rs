//! Validated media inputs, pixel preparation and generated image responses.
mod codec;
mod fit;
mod input;
mod output;
mod preparation;
pub mod request_body;
mod response;
pub use codec::{DecodedImage, decode as decode_validated_image};
pub use input::EncodedImage;
pub use output::{GeneratedImage, GeneratedImageSource, ImageMediaFormat};
pub use preparation::{
    DefaultImagePreprocessor, ImagePreparation, ImagePreprocessor, PreparedImage, prepare_image,
};
pub use response::{
    decode_generated_image_response, decode_generated_image_response_with_policy,
    decode_generated_images, decode_generated_images_with_policy, validated_generated_image,
};

#[cfg(test)]
mod tests;
