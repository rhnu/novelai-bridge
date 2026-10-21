use super::{
    GenerateImageRequest, NormalizationDiagnostic, NormalizationPolicy, PreparedGeneration,
    ResolvedPrompts, normalize, prepare,
};
use crate::{BridgeError, ImagePreprocessor};

/// Validated effective input with conditioning text assembled exactly once.
///
/// Created only by normalization. Borrowed fields are immutable; use
/// [`Self::into_request`] and normalize again to edit.
///
/// Image inputs were validated at construction. Planning does not transform their cached pixels
/// or select a random seed, so preparation and remote transport can still fail independently.
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
    pub(super) request: GenerateImageRequest,
    pub(super) prompts: ResolvedPrompts,
    pub(super) diagnostics: Vec<NormalizationDiagnostic>,
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
    pub const fn request(&self) -> &GenerateImageRequest { &self.request }

    /// Cached effective text shared by token counting, wire construction and observation.
    #[must_use]
    pub const fn resolved_prompts(&self) -> &ResolvedPrompts { &self.prompts }

    /// Decisions made while constructing this plan, in normalization order.
    #[must_use]
    pub fn diagnostics(&self) -> &[NormalizationDiagnostic] { &self.diagnostics }

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
    pub fn into_request(self) -> GenerateImageRequest { self.request }

    /// Resolves the seed and prepares image payloads without sending a request.
    ///
    /// # Errors
    ///
    /// Returns image preparation errors.
    pub fn prepare(self) -> Result<PreparedGeneration, BridgeError> { prepare::prepare(self) }

    /// Resolves the seed and prepares image payloads with a caller-provided pixel transformer.
    ///
    /// # Errors
    ///
    /// Returns errors reported by the preprocessor.
    pub fn prepare_with(
        self,
        preprocessor: &dyn ImagePreprocessor,
    ) -> Result<PreparedGeneration, BridgeError> {
        prepare::prepare_with(self, preprocessor)
    }
}
