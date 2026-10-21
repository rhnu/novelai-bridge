//! Effective generation planning, image preparation and wire encoding.
mod builder;
mod characters;
mod conditioning;
mod diagnostics;
mod geometry;
mod input;
mod normalize;
mod plan;
mod prepare;
mod pricing;
mod references;
mod request;
mod result;
mod seed;
#[cfg(test)]
pub mod test_support;
#[cfg(feature = "tokenizer")]
mod token_usage;
mod wire;

pub use builder::GenerateImageRequestBuilder;
pub use characters::{Character, CharacterPosition, CharacterSet};
pub use conditioning::{ResolvedCharacterPrompt, ResolvedPrompts};
pub use diagnostics::{NormalizationChange, NormalizationDiagnostic};
pub use geometry::GenerationGeometry;
pub use input::{GenerationInput, ImageToImageInput, InpaintImageToImage, InpaintingInput};
pub use plan::NormalizedGeneration;
pub use prepare::{GenerationDelivery, PreparedGeneration};
pub use references::{CharacterReference, CharacterReferenceType, ReferenceSet};
pub use request::{
    GenerateImageRequest, GenerateImageStreamRequest, GenerationCore, GuidanceParams, OutputParams,
};
pub use result::GenerateImageResult;
#[cfg(feature = "tokenizer")]
pub use token_usage::{CharacterTokenUsage, PromptField, PromptTokenUsage};
