//! Generation planning is independent of transport and image preparation.

mod normalize;
mod prepare;

use crate::{BridgeError, GenerateImageRequest, ResolvedPrompts};
pub use prepare::PreparedGeneration;

/// How unsupported combinations and out-of-range values are handled.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum NormalizationPolicy {
    /// Reject invalid ranges and unsupported features.
    Reject,
    /// Clamp ranges and remove unsupported features, reporting every change.
    Normalize,
}

/// Why a field differs from the caller's input.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum NormalizationChange {
    /// A numeric range, canvas grid or coordinate grid changed a value.
    Adjusted {
        /// Original finite value.
        from: f64,
        /// Effective value.
        to: f64,
    },
    /// The selected model does not support the field.
    Unsupported,
    /// An unsupported value was replaced by a supported default.
    Fallback,
    /// An incompatible field was removed.
    Conflict {
        /// Field retained in preference to this field.
        retained_field: &'static str,
    },
    /// This enabled character exceeded the model's limit.
    CharacterLimit {
        /// Original index, including disabled entries.
        index: usize,
        /// Maximum number of enabled characters.
        limit: u32,
    },
}

/// A deterministic, de-identified record of a normalization decision.
///
/// Contains no prompt text, image payload or credentials.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NormalizationDiagnostic {
    /// Field path in the original request.
    pub field: String,
    /// Decision applied to the field.
    pub change: NormalizationChange,
}

/// Validated effective input with conditioning text assembled exactly once.
///
/// Created only by normalization. Borrowed fields are immutable; use
/// [`Self::into_request`] and normalize again to edit.
///
/// Planning does not decode images or select a random seed. A successful plan therefore does
/// not guarantee that image preparation or a remote request will succeed.
///
/// ```compile_fail
/// use novelai_bridge::{GenerateImageRequest, Model};
/// let mut plan = GenerateImageRequest::builder("cat", Model::NaiDiffusion45Full)
///     .build().normalize().unwrap();
/// plan.request().sampling.steps = 0;
/// ```
///
/// ```compile_fail
/// use novelai_bridge::NormalizedGeneration;
/// let plan: NormalizedGeneration = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedGeneration {
    request: GenerateImageRequest,
    prompts: ResolvedPrompts,
    diagnostics: Vec<NormalizationDiagnostic>,
}

impl GenerateImageRequest {
    /// Normalizes this input according to `strict_mode`.
    ///
    /// # Errors
    ///
    /// Returns local range, capability or request-shape errors.
    pub fn normalize(self) -> Result<NormalizedGeneration, BridgeError> {
        let policy = if self.strict_mode {
            NormalizationPolicy::Reject
        } else {
            NormalizationPolicy::Normalize
        };
        self.normalize_with(policy)
    }

    /// Normalizes this input with an explicit policy, overriding `strict_mode`.
    ///
    /// # Errors
    ///
    /// Returns local range, capability or request-shape errors.
    pub fn normalize_with(
        self,
        policy: NormalizationPolicy,
    ) -> Result<NormalizedGeneration, BridgeError> {
        normalize::normalize(self, policy)
    }

    /// Normalizes this request and returns its effective conditioning text.
    ///
    /// Reuse [`NormalizedGeneration::resolved_prompts`] for repeated queries without cloning
    /// the request or rebuilding its prompts.
    ///
    /// # Errors
    ///
    /// Returns local normalization errors.
    pub fn resolve_prompts(&self) -> Result<ResolvedPrompts, BridgeError> {
        Ok(self.clone().normalize()?.prompts)
    }
}

impl NormalizedGeneration {
    /// Immutable effective input. Disabled characters retain their original, unvalidated data.
    #[must_use]
    pub const fn request(&self) -> &GenerateImageRequest {
        &self.request
    }

    /// Cached effective text shared by token counting, wire construction and observation.
    #[must_use]
    pub const fn resolved_prompts(&self) -> &ResolvedPrompts {
        &self.prompts
    }

    /// Decisions made while constructing this plan, in normalization order.
    #[must_use]
    pub fn diagnostics(&self) -> &[NormalizationDiagnostic] {
        &self.diagnostics
    }

    /// Policy used to construct this plan.
    #[must_use]
    pub const fn policy(&self) -> NormalizationPolicy {
        if self.request.strict_mode {
            NormalizationPolicy::Reject
        } else {
            NormalizationPolicy::Normalize
        }
    }

    /// Returns editable effective input, discarding the cached prompts and diagnostics.
    #[must_use]
    pub fn into_request(self) -> GenerateImageRequest {
        self.request
    }

    /// Resolves the seed and prepares image payloads without sending a request.
    ///
    /// # Errors
    ///
    /// Returns payload decoding or image preparation errors.
    pub fn prepare(self) -> Result<PreparedGeneration, BridgeError> {
        prepare::prepare(self)
    }
}
