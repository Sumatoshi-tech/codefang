//! `cf-analyzer-burndown` — burndown analyzer metrics + serialization.
//!
//! The full history walk lives in the command layer; [`metrics`] owns the
//! `ComputedMetrics` model, the sparse→dense history densification, and the
//! serialization emitted for the `json` / `yaml` / `bin` machine formats.
//!
//! Compatibility: output bytes are pinned against the reference implementation
//! by `rust/tests/compat`.

pub mod metrics;

pub use metrics::{
    compute_global_metrics, group_sparse_history, AggregateData, ComputedMetrics, DenseHistory,
    SparseHistory, SurvivalData,
};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-analyzer-burndown";
