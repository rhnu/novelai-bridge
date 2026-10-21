use serde::{Deserialize, Serialize};

use crate::VibeTransferConfig;

/// Character reference behavior used by director references.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharacterReferenceType {
    /// Character-only reference.
    Character,
    /// Style-only reference.
    Style,
    /// Combined character and style reference.
    CharacterAndStyle,
    /// Costume reference.
    Costume,
    /// Delta reference.
    Delta,
}

impl CharacterReferenceType {
    /// Returns the caption value sent as `director_reference_descriptions[].caption.base_caption`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Style => "style",
            Self::CharacterAndStyle => "character&style",
            Self::Costume => "costume",
            Self::Delta => "delta",
        }
    }
}

crate::parameters::serde_string_enum!(
    CharacterReferenceType,
    [
        Self::Character,
        Self::Style,
        Self::CharacterAndStyle,
        Self::Costume,
        Self::Delta
    ]
);

/// Character reference input used by director references.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CharacterReference {
    /// Validated reference image.
    pub image: crate::EncodedImage,
    /// How the reference should be interpreted.
    pub reference_type: CharacterReferenceType,
    /// Amount of information extracted, normalized to 0–1; defaults to one.
    #[serde(default = "default_information_extracted")]
    pub information_extracted: f32,
    /// Finite, signed fidelity used to derive `1 - fidelity` secondary strength.
    pub fidelity: f32,
    /// Finite, signed reference strength.
    pub strength: f32,
}

impl CharacterReference {
    /// Creates a reference with information extraction, fidelity and strength set to one.
    #[must_use]
    pub const fn new(image: crate::EncodedImage, reference_type: CharacterReferenceType) -> Self {
        Self {
            image,
            reference_type,
            information_extracted: 1.0,
            fidelity: 1.0,
            strength: 1.0,
        }
    }
}

const fn default_information_extracted() -> f32 { 1.0 }

/// Reference images attached to a generation request.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceSet {
    /// Optional vibe transfer configuration.
    pub vibe_transfer: Option<VibeTransferConfig>,
    /// Optional director-style character references.
    pub character_references: Vec<CharacterReference>,
}

impl ReferenceSet {
    /// Returns `true` when no reference of any kind is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.character_references.is_empty()
            && self
                .vibe_transfer
                .as_ref()
                .is_none_or(|vibe| vibe.references.is_empty())
    }
}
