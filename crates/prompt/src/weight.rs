use crate::PromptWeight;

impl PromptWeight {
    pub(crate) const ONE: Self = Self::Exact { value: 1.0 };

    pub(crate) const fn bounds(self) -> Option<(f64, f64)> {
        match self {
            Self::Exact { value } => Some((value, value)),
            Self::Range { min, max } => Some((min, max)),
            Self::Unknown => None,
        }
    }

    pub(crate) fn scale(self, factor: f64) -> Self {
        let Some((min, max)) = self.bounds() else {
            return Self::Unknown;
        };
        Self::from_bounds(min * factor, max * factor)
    }

    pub(crate) fn join(self, other: Self) -> Self {
        match (self.bounds(), other.bounds()) {
            (Some((a, b)), Some((c, d))) => Self::from_bounds(a.min(c), b.max(d)),
            _ => Self::Unknown,
        }
    }

    #[expect(
        clippy::float_cmp,
        reason = "Exact equality preserves any representable branch uncertainty"
    )]
    fn from_bounds(min: f64, max: f64) -> Self {
        if !min.is_finite() || !max.is_finite() {
            Self::Unknown
        } else if min == max {
            Self::Exact { value: min }
        } else {
            Self::Range { min, max }
        }
    }
}
