use crate::GenerationField;

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
