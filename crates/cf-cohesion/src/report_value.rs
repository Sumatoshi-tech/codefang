//! The scalar report map consumed by the history `quality` analyzer.
//!
//! [`crate::FileReport::to_report`] produces it; consumers read the scalars
//! by key with a strict type assertion (an `Int` is never coerced to a float).

use std::collections::BTreeMap;

/// A scalar value inside a [`Report`].
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// An integer (`total_functions`).
    Int(i64),
    /// A float (`lcom`, `cohesion_score`, `function_cohesion`).
    Float(f64),
    /// A string (`message`).
    Str(String),
}

impl ReportValue {
    /// The value as an `f64` if it is [`ReportValue::Float`], else `None`.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ReportValue::Float(v) => Some(*v),
            _ => None,
        }
    }
}

/// The per-file scalar report, keyed by metric name.
pub type Report = BTreeMap<String, ReportValue>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_float_is_a_strict_type_assertion() {
        assert_eq!(ReportValue::Float(0.5).as_float(), Some(0.5));
        assert_eq!(ReportValue::Int(1).as_float(), None);
        assert_eq!(ReportValue::Str("x".into()).as_float(), None);
    }
}
