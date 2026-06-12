//! `cf-halstead` — Halstead complexity metrics analyzer.
//!
//! The full per-function/per-file report builder, formatter, visitor, and
//! aggregator for the static pipeline live in
//! `cf-commands/src/handlers/static_halstead.rs`. The module surface exposed
//! here is the part the **quality** history analyzer consumes: the
//! operator/operand [`detector`], the derived-metric [`calculator`], and the
//! standalone file-level [`analyze`].
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by the differential gate in `rust/tests/compat`.

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-halstead";

/// Analyzer name as it appears in reports (`analyzer_name`).
pub const ANALYZER_NAME: &str = "halstead";

/// Minimum total tokens (operators + operands) before the count-min-sketch
/// path activates. The CMS path only affects the `estimated_total_*` fields,
/// which the quality analyzer does not read.
pub const CMS_TOKEN_THRESHOLD: i64 = 1000;

/// CMS approximation error bound.
pub const CMS_EPSILON: f64 = 0.001;

/// CMS failure probability.
pub const CMS_DELTA: f64 = 0.01;

/// Maximum UAST traversal depth for function discovery.
pub const MAX_DEPTH: i64 = 10;

pub mod calculator;
pub mod detector;
pub mod standalone;

pub use standalone::{analyze, FileHalstead};
