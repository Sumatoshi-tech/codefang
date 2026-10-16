//! `cf-analyzer-burndown` — burndown report model, survival arithmetic and
//! serialization.
//!
//! The history walk (line-survival treaps per file, identity, ticks) lives in
//! the command layer; this crate turns the walk's sparse histories into the
//! report: [`metrics`] owns the [`ComputedMetrics`] model and its machine
//! encoding, [`survival`] the sparse → dense densification and every figure
//! derived from it (global, per-file and per-developer survival, the
//! developer interaction matrix).

pub mod metrics;
pub mod survival;

pub use metrics::{
    AggregateData, ComputedMetrics, DeveloperSurvival, FileOwnership, FileSurvival, Interaction,
    SurvivalData,
};
pub use survival::{
    compute_metrics, compute_survival_sample, find_peak_lines, group_sparse_history, DenseHistory,
    Grid, MetricsInput, SparseHistory,
};
