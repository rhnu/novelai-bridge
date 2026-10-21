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

crate::parameters::serde_string_enum!(CharacterReferenceType, [Self::Character, Self::Style, Self::CharacterAndStyle, Self::Costume, Self::Delta]);

/// Character reference input used by director references.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CharacterReference {
    /// Validated reference image.
    pub image: crate::EncodedImage,
    /// How the reference should be interpreted.
    pub reference_type: CharacterReferenceType,
    /// Fidelity value used to derive secondary strength.
    pub fidelity: f32,
    /// Overall reference strength.
    pub strength: f32,
}

/// Reference images attached to a generation request.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
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
