//! Dynamic report value model — the Rust equivalent of Go's `analyze.Report`
//! (`map[string]any`) restricted to the value shapes the cohesion analyzer produces.
//!
//! The Go analyzer builds an intermediate `analyze.Report = map[string]any` from
//! [`crate::analyzer::Analyzer::analyze`], which is then re-parsed by
//! [`crate::metrics::compute_all_metrics`] (the function that produces the actual
//! machine-format output). We model that intermediate map explicitly so the
//! round-trip (`build_result` -> `parse_report_data`) is faithful and testable
//! without dragging in the full cross-crate `analyze.Report` type while it is still
//! being ported.
//!
//! # Seam
//!
//! In the integrated workspace this should be replaced by / converted to the shared
//! `cf-analyze::Report` type. The shape here is intentionally a strict subset:
//! the keys and value types are exactly those the Go cohesion code reads and writes
//! (see `metrics.go::ParseReportData`). See the crate todos.

use std::collections::BTreeMap;

/// A value inside a cohesion [`Report`].
///
/// Mirrors the concrete dynamic types stored in the Go `map[string]any`: `int`,
/// `float64`, `string`, and `[]map[string]any` (the per-function table).
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// A Go `int` (e.g. `total_functions`).
    Int(i64),
    /// A Go `float64` (e.g. `lcom`, `cohesion_score`).
    Float(f64),
    /// A Go `string` (e.g. `message`, `analyzer_name`).
    Str(String),
    /// A list of string-keyed maps (the `functions` table).
    ///
    /// Each inner map preserves the per-function item keys produced by
    /// `convertCohesionFunctionItems` in the Go code.
    Functions(Vec<BTreeMap<String, ReportValue>>),
}

impl ReportValue {
    /// Returns the value as an `i64` if it is [`ReportValue::Int`] (Go
    /// `report[k].(int)`), else `None`.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ReportValue::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the value as an `f64` if it is [`ReportValue::Float`] (Go
    /// `report[k].(float64)`), else `None`.
    ///
    /// NOTE: this is a strict type assertion mirroring Go's `.(float64)`. An `Int`
    /// is **not** coerced, exactly like Go's type switch — the cohesion scalars are
    /// always stored as `float64`.
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

/// The analyzer's intermediate result map (Go `analyze.Report`).
///
/// A `BTreeMap` so iteration / encoding is byte-key-sorted by default, matching Go's
/// `map[string]any` JSON encoding rule (DESIGN §2.2).
pub type Report = BTreeMap<String, ReportValue>;
