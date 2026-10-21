use std::sync::Arc;

use base64::{Engine, engine::general_purpose::STANDARD};
use novelai_bridge::{
    BridgeError, Character, CharacterPosition, CharacterReference, CharacterReferenceType,
    CharacterSet, Client, ClientConfig, EncodedImage, GenerateImageRequest, GeneratedImageMetadata,
    GenerationField, GuidanceField, GuidanceParams, ImageFormat, ImageSize, ImageToImageInput,
    InpaintingInput, InvalidRequest, Model, QualityPreset, ReferenceSet, RequestField, UcPreset,
    ValidationPolicy, VibeEncoding, VibeReference, VibeTransferConfig,
};
use serde_json::{Value, json};

use crate::support::{
    MockTransport, create_png_base64, create_png_with_comment, create_zip_with_pngs,
    create_zip_with_single_png,
};

fn request(prompt: &str) -> GenerateImageRequest {
    GenerateImageRequest::builder(prompt, Model::default()).build()
}

fn request_for(prompt: &str, model: Model) -> GenerateImageRequest {
    GenerateImageRequest::builder(prompt, model).build()
}

fn png_data_url(width: u32, height: u32) -> EncodedImage {
    EncodedImage::from_base64(&create_png_base64(width, height)).expect("fixture image is valid")
}

fn decoded_dimensions(value: &Value) -> (u32, u32) {
    let encoded = value.as_str().expect("base64 image payload");
    let bytes = STANDARD.decode(encoded).expect("decode base64");
    let image = image::load_from_memory(&bytes).expect("decode image");
    (image.width(), image.height())
}

mod characters;
mod image_inputs;
mod normalization;
mod observation;
mod ordinary;
mod prompts;
mod stream;
