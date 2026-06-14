//! Dynamic report value model, restricted to the value shapes the cohesion
//! analyzer produces.
//!
//! The analyzer builds an intermediate dynamic [`Report`] map from
//! [`crate::analyzer::Analyzer::analyze`], which is then re-parsed by
//! [`crate::metrics::compute_all_metrics`] (the function that produces the
//! actual machine-format output). Modeling that intermediate map explicitly
//! keeps the round-trip (`build_result` -> `parse_report_data`) faithful and
//! testable in isolation. The shape is intentionally a strict subset of the
//! shared dynamic-report type: exactly the keys and value types the cohesion
//! pipeline reads and writes.

use std::collections::BTreeMap;

/// A value inside a cohesion [`Report`].
///
/// The concrete dynamic types: integer, float, string, and a list of
/// string-keyed maps (the per-function table). The integer/float distinction
/// is significant: scalar parsing is a strict type assertion, never a
/// coercion.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// An integer (e.g. `total_functions`).
    Int(i64),
    /// A float (e.g. `lcom`, `cohesion_score`).
    Float(f64),
    /// A string (e.g. `message`, `analyzer_name`).
    Str(String),
    /// A list of string-keyed maps (the `functions` table), preserving the
    /// per-function item keys.
    Functions(Vec<BTreeMap<String, ReportValue>>),
}

impl ReportValue {
    /// Returns the value as an `i64` if it is [`ReportValue::Int`], else
    /// `None`.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ReportValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the value as an `f64` if it is [`ReportValue::Float`], else
    /// `None`.
    ///
    /// NOTE: this is a strict type assertion (report-format contract). An
    /// `Int` is **not** coerced — the cohesion scalars are always stored as
    /// floats.
    #[must_use]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ReportValue::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the value as a `&str` if it is [`ReportValue::Str`], else `None`.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ReportValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the per-function table if this is [`ReportValue::Functions`], else
    /// `None`.
    #[must_use]
    pub fn as_functions(&self) -> Option<&[BTreeMap<String, ReportValue>]> {
        match self {
            ReportValue::Functions(f) => Some(f),
            _ => None,
        }
    }
}

/// The analyzer's intermediate result map.
///
/// A `BTreeMap` so iteration / encoding is byte-key-sorted by default,
/// matching the report contract's map-key encoding rule (DESIGN §2.2).
pub type Report = BTreeMap<String, ReportValue>;
