use std::fmt;

/// How unsupported combinations and numeric ranges are handled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NormalizationPolicy {
    /// Reject unsupported features and values outside their allowed ranges.
    Reject,
    /// Clamp values and remove unsupported features, reporting changes.
    Normalize,
}

/// A coordinate or text within a character entry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharacterField { Entry, Prompt, X, Y }

/// Numeric properties of a character reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharacterReferenceField { Fidelity, Strength }

/// Model-gated sampling guidance.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GuidanceField { VarietyBoost, Decrisp, Smea }

/// Image-to-image input properties.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageInputField { Mask, Strength, Noise }

/// A typed location in the editable generation request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenerationField {
    Prompt, Size, Width, Height, Samples, Quality, TransparentBackground, FurryMode,
    Steps, Scale, CfgRescale, NoiseSchedule,
    Characters,
    Character { index: usize, field: CharacterField },
    Guidance(GuidanceField),
    ImageInput(ImageInputField),
    CharacterReferences,
    CharacterReference { index: usize, field: CharacterReferenceField },
    VibeTransfer,
    VibeStrength,
    VibeReferences,
    VibeReferenceStrength(usize),
}

impl fmt::Display for GenerationField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = match self {
            Self::Prompt => "core.prompt",
            Self::Size => "core.size",
            Self::Width => "core.size.width",
            Self::Height => "core.size.height",
            Self::Samples => "core.n_samples",
            Self::Quality => "core.quality",
            Self::TransparentBackground => "core.transparent_background",
            Self::FurryMode => "core.furry_mode",
            Self::Steps => "sampling.steps",
            Self::Scale => "sampling.scale",
            Self::CfgRescale => "sampling.cfg_rescale",
            Self::NoiseSchedule => "sampling.noise_schedule",
            Self::Characters => "characters",
            Self::Guidance(GuidanceField::VarietyBoost) => "guidance.variety_boost",
            Self::Guidance(GuidanceField::Decrisp) => "guidance.decrisp",
            Self::Guidance(GuidanceField::Smea) => "guidance.smea",
            Self::ImageInput(ImageInputField::Mask) => "img2img.mask",
            Self::ImageInput(ImageInputField::Strength) => "img2img.strength",
            Self::ImageInput(ImageInputField::Noise) => "img2img.noise",
            Self::CharacterReferences => "references.character_references",
            Self::VibeTransfer => "references.vibe_transfer",
            Self::VibeStrength => "references.vibe_transfer.strength",
            Self::VibeReferences => "references.vibe_transfer.references",
            Self::Character { index, field } => {
                let suffix = match field { CharacterField::Entry => "", CharacterField::Prompt => ".prompt", CharacterField::X => ".position.x", CharacterField::Y => ".position.y" };
                return write!(formatter, "characters.characters[{index}]{suffix}");
            }
            Self::CharacterReference { index, field } => {
                let suffix = match field { CharacterReferenceField::Fidelity => "fidelity", CharacterReferenceField::Strength => "strength" };
                return write!(formatter, "references.character_references[{index}].{suffix}");
            }
            Self::VibeReferenceStrength(index) => {
                return write!(formatter, "references.vibe_transfer.references[{index}].strength");
            }
        };
        formatter.write_str(path)
    }
}

/// Why effective input differs from editable input.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum NormalizationChange {
    /// Numeric or coordinate canonicalization.
    Adjusted { from: f64, to: f64 },
    /// Unsupported by the selected model.
    Unsupported,
    /// Replaced with a supported fallback.
    Fallback,
    /// Another field takes precedence.
    Conflict { retained_field: GenerationField },
    /// Enabled character exceeded the model limit.
    CharacterLimit { index: usize, limit: u32 },
}

/// A deterministic normalization decision without prompt text or image data.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NormalizationDiagnostic {
    /// Location in the original editable request.
    pub field: GenerationField,
    /// Applied decision.
    pub change: NormalizationChange,
}
