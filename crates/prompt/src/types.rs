use serde::{Deserialize, Serialize};

/// Syntax capabilities supplied by the SDK's model descriptor, rather than inferred from names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Independent provider syntax capabilities"
)]
pub struct PromptSyntaxProfile {
    pub bracket_weights: bool,
    pub numeric_weights: bool,
    pub negative_weights: bool,
    pub legacy_mixing: bool,
    pub character_separators: bool,
    pub randomizer: bool,
    pub text_rendering: bool,
    pub interaction_tags: bool,
}

/// All positions are half-open UTF-16 code-unit offsets in the original input.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PromptAnalysis {
    pub tokens: Vec<PromptToken>,
    pub spans: Vec<PromptSemanticSpan>,
    pub diagnostics: Vec<PromptDiagnostic>,
}

/// A lossless source token. Concatenating `text` reproduces the complete input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PromptToken {
    pub from: u32,
    pub to: u32,
    pub text: String,
    pub kind: PromptTokenKind,
    /// Parsed numeric value for numeric emphasis or a legacy mixing suffix.
    pub value: Option<f64>,
}

/// Lexical roles; ordinary quotes have no special role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PromptTokenKind {
    Text,
    Escape,
    Strengthen,
    Weaken,
    NumericWeight,
    WeightReset,
    MixSeparator,
    MixWeight,
    CharacterSeparator,
    RandomOpen,
    RandomSeparator,
    RandomClose,
    TextMarker,
    RenderedText,
    UncertainText,
    Interaction,
}

/// Text with effective emphasis; random branches are analyzed without choosing an option.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PromptSemanticSpan {
    pub from: u32,
    pub to: u32,
    pub kind: PromptSemanticKind,
    pub weight: PromptWeight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PromptSemanticKind {
    Text,
    RenderedText,
    UncertainText,
}

/// A range encloses all possible emphasis weights, without claiming every value is attainable.
/// Unknown represents non-finite arithmetic or ambiguous literal rendering.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PromptWeight {
    Exact { value: f64 },
    Range { min: f64, max: f64 },
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PromptDiagnostic {
    pub from: u32,
    pub to: u32,
    pub code: PromptDiagnosticCode,
    pub severity: PromptDiagnosticSeverity,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PromptDiagnosticCode {
    UnsupportedNumericWeight,
    UnsupportedNegativeWeight,
    NonFiniteWeight,
    UnclosedRandomizer,
    UncertainTextRendering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum PromptDiagnosticSeverity {
    Warning,
}
