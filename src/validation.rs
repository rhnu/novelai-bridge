use serde::{Deserialize, Serialize};

/// Handling of unsupported input and values outside supported ranges.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationPolicy {
    /// Reject unsupported features and out-of-range values.
    Reject,
    /// Canonicalize supported values and report changes.
    #[default]
    Normalize,
}

impl ValidationPolicy {
    #[must_use]
    pub const fn is_reject(self) -> bool { matches!(self, Self::Reject) }
}

/// A numeric adjustment performed under normalize policy.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ValidationAdjustment {
    pub field: crate::RequestField,
    pub from: f64,
    pub to: f64,
}
