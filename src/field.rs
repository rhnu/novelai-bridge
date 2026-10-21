//! Typed editable locations, independent of optional execution capabilities.
use std::fmt;

/// A coordinate or text within a character entry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharacterField {
    Entry,
    Prompt,
    NegativePrompt,
    X,
    Y,
}

/// Numeric properties of a character reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CharacterReferenceField {
    Image,
    Fidelity,
    Strength,
}

/// Model-gated sampling guidance.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GuidanceField {
    VarietyBoost,
    Decrisp,
    Smea,
}

/// Image-to-image input properties.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImageInputField {
    Image,
    Mask,
    Strength,
    Noise,
}

/// A typed location in the editable generation request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenerationField {
    Prompt,
    NegativePrompt,
    Size,
    Width,
    Height,
    Samples,
    Quality,
    TransparentBackground,
    FurryMode,
    Steps,
    Scale,
    CfgRescale,
    NoiseSchedule,
    Characters,
    Character {
        index: usize,
        field: CharacterField,
    },
    Guidance(GuidanceField),
    ImageInput(ImageInputField),
    CharacterReferences,
    CharacterReference {
        index: usize,
        field: CharacterReferenceField,
    },
    VibeTransfer,
    VibeStrength,
    VibeReferences,
    VibeReferenceStrength(usize),
}

impl fmt::Display for GenerationField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = match self {
            Self::Prompt => "core.prompt",
            Self::NegativePrompt => "core.negative_prompt",
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
            Self::ImageInput(ImageInputField::Image) => "operation.input.image",
            Self::ImageInput(ImageInputField::Mask) => "operation.input.mask",
            Self::ImageInput(ImageInputField::Strength) => "operation.input.strength",
            Self::ImageInput(ImageInputField::Noise) => "operation.input.noise",
            Self::CharacterReferences => "references.character_references",
            Self::VibeTransfer => "references.vibe_transfer",
            Self::VibeStrength => "references.vibe_transfer.strength",
            Self::VibeReferences => "references.vibe_transfer.references",
            Self::Character { index, field } => {
                let suffix = match field {
                    CharacterField::Entry => "",
                    CharacterField::Prompt => ".prompt",
                    CharacterField::NegativePrompt => ".negative_prompt",
                    CharacterField::X => ".position.x",
                    CharacterField::Y => ".position.y",
                };
                return write!(formatter, "characters.characters[{index}]{suffix}");
            },
            Self::CharacterReference { index, field } => {
                let suffix = match field {
                    CharacterReferenceField::Image => "image",
                    CharacterReferenceField::Fidelity => "fidelity",
                    CharacterReferenceField::Strength => "strength",
                };
                return write!(
                    formatter,
                    "references.character_references[{index}].{suffix}"
                );
            },
            Self::VibeReferenceStrength(index) => {
                return write!(
                    formatter,
                    "references.vibe_transfer.references[{index}].strength"
                );
            },
        };
        formatter.write_str(path)
    }
}

/// A stable location shared by validation, preprocessing and diagnostics.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RequestField {
    Generation(GenerationField),
    Image,
    PreparedImage,
    MetadataImage,
    MetadataVibe,
    VibeEncoding,
    VibeInformationExtracted,
    DirectorPrompt,
    DirectorDefry,
    StreamDelivery,
    EncodeVibe,
}

impl From<GenerationField> for RequestField {
    fn from(value: GenerationField) -> Self { Self::Generation(value) }
}

impl fmt::Display for RequestField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = match self {
            Self::Generation(field) => return field.fmt(f),
            Self::Image => "image",
            Self::PreparedImage => "prepared_image",
            Self::MetadataImage => "metadata.image",
            Self::MetadataVibe => "metadata.vibe",
            Self::VibeEncoding => "vibe_encoding",
            Self::VibeInformationExtracted => "information_extracted",
            Self::DirectorPrompt => "command.prompt",
            Self::DirectorDefry => "command.defry",
            Self::StreamDelivery => "delivery.stream",
            Self::EncodeVibe => "encode_vibe",
        };
        f.write_str(path)
    }
}
