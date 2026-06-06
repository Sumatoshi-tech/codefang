//! `cf-analyzer-burndown` — burndown analyzer metrics + serialization.
//!
//! Port target documented in specs/rust-rewrite/DESIGN.md §1. The full history
//! walk is still pending; [`metrics`] owns the byte-identity-critical
//! `ComputedMetrics` serialization that `BaseHistoryAnalyzer.Serialize` emits for
//! the `json` / `yaml` / `bin` machine formats.
#![allow(dead_code)]

pub mod metrics;

pub use metrics::{
    compute_global_metrics, group_sparse_history, AggregateData, ComputedMetrics, DenseHistory,
    SparseHistory, SurvivalData,
};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-analyzer-burndown";
