use super::{GenerationMode, ModelDescriptor, VibeInputPath};

/// Which reference groups the caller intends to use; does not include payload data.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ReferenceIntent {
    /// At least one vibe reference.
    pub vibe: bool,
    /// At least one character reference.
    pub character: bool,
}

/// A reference group's availability for one request context.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferenceAvailability {
    /// The caller did not request this group.
    NotRequested,
    /// Supported by this model, implementation and request combination.
    Available,
    /// No provider capability is registered.
    Unsupported,
    /// The provider supports it, but this bridge does not implement its input path.
    InputNotImplemented,
    /// The model or reference profile does not support this generation mode.
    UnsupportedMode,
    /// Character references take precedence over vibe references.
    ConflictWithCharacterReference,
}

impl ReferenceAvailability {
    /// Whether the group can be retained in this request.
    #[must_use]
    pub const fn is_available(self) -> bool { matches!(self, Self::Available) }
}

/// Pure reference-combination decision, shared by UI queries and normalization.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReferenceResolution {
    /// Vibe-reference decision.
    pub vibe: ReferenceAvailability,
    /// Character-reference decision.
    pub character: ReferenceAvailability,
}

impl ModelDescriptor {
    /// Maximum simultaneous samples for the effective pricing canvas.
    #[must_use]
    pub const fn sample_limit(&self, size: crate::ImageSize) -> u32 {
        let pixels = size.pixels();
        if pixels == 0 || pixels > self.constraints.canvas.max_pixels {
            return 0;
        }
        let limit = if pixels <= 360_448 {
            8
        } else if pixels <= 409_600 {
            6
        } else {
            4
        };
        if limit < self.constraints.samples.max {
            limit
        } else {
            self.constraints.samples.max
        }
    }

    /// Selects the API identity for a supported mode.
    #[must_use]
    pub const fn model_id_for(&self, mode: GenerationMode) -> Option<&'static str> {
        match mode {
            GenerationMode::Inpainting => match self.support.inpainting {
                Some(profile) => Some(profile.model_id),
                None => None,
            },
            GenerationMode::TextToImage | GenerationMode::ImageToImage => {
                Some(self.identity.api_id)
            },
        }
    }

    /// Whether the bridge implements encode-vibe for this model.
    ///
    /// Provider-only legacy vibe capability does not enable this operation.
    #[must_use]
    pub const fn can_encode_vibe(&self) -> bool {
        match self.support.vibe {
            Some(profile) => matches!(profile.input, VibeInputPath::Encoded),
            None => false,
        }
    }

    /// Resolves reference support and combination rules without payload inspection.
    ///
    /// Character references are resolved first. Unsupported character inputs cannot suppress an
    /// otherwise valid vibe input. When both are valid, character references take precedence.
    /// This describes decisions; Reject/Normalize error handling belongs to request normalization.
    #[must_use]
    pub const fn resolve_references(
        &self,
        mode: GenerationMode,
        intent: ReferenceIntent,
    ) -> ReferenceResolution {
        let character = if intent.character {
            self.character_reference_availability(mode)
        } else {
            ReferenceAvailability::NotRequested
        };
        let vibe = if intent.vibe {
            self.vibe_reference_availability(mode, character)
        } else {
            ReferenceAvailability::NotRequested
        };
        ReferenceResolution { vibe, character }
    }

    const fn character_reference_availability(
        &self,
        mode: GenerationMode,
    ) -> ReferenceAvailability {
        let Some(profile) = self.support.character_reference else {
            return ReferenceAvailability::Unsupported;
        };
        if self.model_id_for(mode).is_none() || !profile.modes.allows(mode) {
            ReferenceAvailability::UnsupportedMode
        } else {
            ReferenceAvailability::Available
        }
    }

    const fn vibe_reference_availability(
        &self,
        mode: GenerationMode,
        character: ReferenceAvailability,
    ) -> ReferenceAvailability {
        let Some(profile) = self.support.vibe else {
            return ReferenceAvailability::Unsupported;
        };
        if matches!(profile.input, VibeInputPath::ProviderOnlyLegacy) {
            return ReferenceAvailability::InputNotImplemented;
        }
        if character.is_available() {
            return ReferenceAvailability::ConflictWithCharacterReference;
        }
        if self.model_id_for(mode).is_none() || !profile.modes.allows(mode) {
            ReferenceAvailability::UnsupportedMode
        } else {
            ReferenceAvailability::Available
        }
    }
}

#[cfg(test)]
mod tests;
