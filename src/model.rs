//! Read-only model registry and pure request-combination rules.
mod presets;
mod profiles;
mod registry;
mod rules;

pub use profiles::*;
pub use rules::*;
use serde::{Deserialize, Serialize};

/// Supported `NovelAI` image models.
///
/// Defaults to [`Model::NaiDiffusion5Full`].
///
/// Inpainting siblings are not separate variants; they are derived from the base model through
/// [`Model::inpainting_id`] when a request carries an inpaint mask.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Model {
    /// `nai-diffusion-5-full`
    #[default]
    NaiDiffusion5Full,
    /// `nai-diffusion-5-curated`
    NaiDiffusion5Curated,
    /// `nai-diffusion-4-5-full`
    NaiDiffusion45Full,
    /// `nai-diffusion-4-5-curated`
    NaiDiffusion45Curated,
    /// `nai-diffusion-4-full`
    NaiDiffusion4Full,
    /// `nai-diffusion-4-curated`
    NaiDiffusion4Curated,
    /// `nai-diffusion-3`
    NaiDiffusion3,
    /// `nai-diffusion-furry-3`
    NaiDiffusion3Furry,
}

impl Model {
    /// Every supported base model, in display order.
    pub const ALL: [Self; 8] = [
        Self::NaiDiffusion5Full,
        Self::NaiDiffusion5Curated,
        Self::NaiDiffusion45Full,
        Self::NaiDiffusion45Curated,
        Self::NaiDiffusion4Full,
        Self::NaiDiffusion4Curated,
        Self::NaiDiffusion3,
        Self::NaiDiffusion3Furry,
    ];

    /// Returns the immutable descriptor from the built-in registry.
    #[must_use]
    pub const fn descriptor(&self) -> &'static ModelDescriptor { registry::descriptor(*self) }

    /// Returns the API model identifier from the registry.
    #[must_use]
    pub const fn as_str(&self) -> &'static str { self.descriptor().identity.api_id }

    /// Returns the registered inpainting sibling, or `None` when unavailable.
    #[must_use]
    pub const fn inpainting_id(&self) -> Option<&'static str> {
        self.descriptor().model_id_for(GenerationMode::Inpainting)
    }

    /// Returns the provider's vibe metadata key, including provider-only input paths.
    ///
    /// This is not a promise that the bridge can send or encode vibe inputs for this model.
    #[must_use]
    pub const fn vibe_model_key(&self) -> Option<&'static str> {
        match self.descriptor().support.vibe {
            Some(profile) => Some(profile.metadata_key),
            None => None,
        }
    }
}

/// Registry-owned model knowledge, grouped by responsibility.
///
/// No public constructor, mutable access or deserialization bypasses the registry. Profiles are
/// inspectable values; modifying a detached copy cannot change the descriptor used by requests.
///
/// ```
/// use novelai_bridge::{GenerationMode, Model, ReferenceIntent};
/// let descriptor = Model::NaiDiffusion45Full.descriptor();
/// assert_eq!(descriptor.prompt().token_limit, 512);
/// let result = descriptor.resolve_references(
///     GenerationMode::ImageToImage,
///     ReferenceIntent {
///         vibe: true,
///         character: true,
///     },
/// );
/// assert!(result.character.is_available());
/// assert!(!result.vibe.is_available());
/// ```
///
/// ```compile_fail
/// use novelai_bridge::Model;
/// Model::NaiDiffusion5Full.descriptor().support().streaming = false;
/// ```
///
/// ```compile_fail
/// use novelai_bridge::ModelDescriptor;
/// let descriptor: ModelDescriptor = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDescriptor {
    identity: ModelIdentity,
    support: ModelSupport,
    constraints: GenerationConstraints,
    defaults: ModelDefaults,
    prompt: PromptProfile,
    sampling: SamplingProfile,
    wire: WireProfile,
    pricing: PricingProfile,
}

impl ModelDescriptor {
    /// API identity of this model.
    #[must_use]
    pub const fn identity(&self) -> &ModelIdentity { &self.identity }

    /// Operation and reference profiles supported by this model.
    #[must_use]
    pub const fn support(&self) -> &ModelSupport { &self.support }

    /// Local generation limits used by normalization.
    #[must_use]
    pub const fn constraints(&self) -> &GenerationConstraints { &self.constraints }

    /// Factory request defaults used by builders.
    #[must_use]
    pub const fn defaults(&self) -> &ModelDefaults { &self.defaults }

    /// Tokenizer, context limit and conditioning options.
    #[must_use]
    pub const fn prompt(&self) -> &PromptProfile { &self.prompt }

    /// Sampling support and noise-schedule fallback rules.
    #[must_use]
    pub const fn sampling(&self) -> &SamplingProfile { &self.sampling }

    /// Protocol shape, version and extensions.
    #[must_use]
    pub const fn wire(&self) -> &WireProfile { &self.wire }

    /// Model-specific pricing policy.
    #[must_use]
    pub const fn pricing(&self) -> &PricingProfile { &self.pricing }
}

impl Serialize for Model {
    fn serialize<Serializer: serde::Serializer>(
        &self,
        serializer: Serializer,
    ) -> Result<Serializer::Ok, Serializer::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'input> Deserialize<'input> for Model {
    fn deserialize<Deserializer: serde::Deserializer<'input>>(
        deserializer: Deserializer,
    ) -> Result<Self, Deserializer::Error> {
        let identifier = String::deserialize(deserializer)?;
        Self::ALL
            .into_iter()
            .find(|model| model.as_str() == identifier)
            .ok_or_else(|| {
                serde::de::Error::custom(format!("unknown model identifier: {identifier}"))
            })
    }
}

#[cfg(test)]
mod tests;
