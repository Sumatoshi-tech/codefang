//! `cf-couples` — file/developer co-change coupling analysis.
//!
//! Port of the Go package `internal/analyzers/couples` (analyzer id
//! `history/couples`, non-sequential / parallelizable). From the commit history
//! it derives two co-occurrence ("coupling") matrices and per-file ownership:
//!
//! - **File coupling** — how often a pair of files changed in the same commit
//!   ([`matrix::compute_files_matrix`], [`metrics::compute_file_coupling`]).
//! - **Developer coupling** — how often a pair of developers touched the same
//!   file ([`matrix::compute_people_matrix`],
//!   [`metrics::compute_developer_coupling`]).
//! - **File ownership** — per-file contributor cardinality via a `HyperLogLog`
//!   sketch keyed by `LittleEndian(devID)`
//!   ([`metrics::compute_file_ownership`]).
//! - **Aggregate** summary statistics ([`metrics::compute_aggregate`]).
//!
//! The coupling-strength formula is code-maat's `co_changes / avg(revs_a,
//! revs_b)` capped at `1.0`, where `revs` is the diagonal (self-change) count.
//!
//! ## Determinism and byte-identity
//!
//! All index assignment flows from byte-sorted name lists ([`sort.Strings`]
//! semantics), so the matrices, sequences, and report are reproducible.
//! Per-commit author-file maps only ever store a count of `1` and accumulate
//! additively, so map iteration order is irrelevant to the result. The HLL
//! sketch is the bit-identical reimplementation from `pkg/alg/hll`.
//!
//! Machine-format report bytes (json/yaml/ndjson/timeseries/compact/bin) are
//! produced via [`cf_gojson`] in [`report`]; this crate never uses `serde_json`
//! for output. See `specs/rust-rewrite/DESIGN.md` §2.
//!
//! [`sort.Strings`]: https://pkg.go.dev/sort#Strings

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod matrix;
pub mod metrics;
pub mod report;
pub mod report_section;
pub mod store;
pub mod tc;

/// Analyzer descriptor constants (`history.go` `Descriptor`).
pub mod descriptor {
    /// Analyzer id.
    pub const ID: &str = "history/couples";
    /// Human-readable description.
    pub const DESCRIPTION: &str = "The result is a square matrix, the value in each cell corresponds to the number of times \
the pair of files appeared in the same commit or pair of developers committed to the same file.";
    /// The analyzer is NOT sequential-only (it can be parallelized).
    pub const SEQUENTIAL: bool = false;
    /// Analyzer name (`Name()`).
    pub const NAME: &str = "Couples";
    /// CLI flag (`Flag()`).
    pub const FLAG: &str = "couples";
}

/// Configuration option keys (`history.go` `ConfigCouples*`).
pub mod config {
    pub const COUPLING_THRESHOLD_HIGH: &str = "Couples.CouplingThresholdHigh";
    pub const OWNERSHIP_FEW_THRESHOLD: &str = "Couples.OwnershipFewThreshold";
    pub const OWNERSHIP_MODERATE_THRESHOLD: &str = "Couples.OwnershipModerateThreshold";
    pub const BATCH_COUPLING_THRESHOLD: &str = "Couples.BatchCouplingThreshold";
    pub const HLL_PRECISION: &str = "Couples.HLLPrecision";
    pub const TOP_K_PER_FILE: &str = "Couples.TopKPerFile";
    pub const MIN_EDGE_WEIGHT: &str = "Couples.MinEdgeWeight";
}

/// Maximum number of files in a commit to consider for coupling analysis
/// (Go: `CouplesMaximumMeaningfulContextSize`). Larger changesets are bulk
/// operations (vendor updates, mass renames, formatting) that produce noise.
pub const COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE: usize = 200;

/// Splits an identity string `"name|email"` into its components
/// (Go: `identity.SplitIdentity`). Thin re-export over [`cf_identity`] so the
/// crate's metric code can call `crate::split_identity`.
#[must_use]
pub fn split_identity(s: &str) -> (String, String) {
    cf_identity::split_identity(s)
}

pub use metrics::{
    bucket_ownership, bucket_ownership_with_thresholds, compute_aggregate, compute_all_metrics,
    compute_all_metrics_with_options, compute_developer_coupling, compute_file_coupling,
    compute_file_ownership, filter_top_devs, sort_ownership_by_risk, AggregateData, ComputedMetrics,
    DeveloperCouplingData, FileCouplingData, FileOwnershipData, MetricOptions, OwnershipBucket,
    ReportData, COUPLING_THRESHOLD_HIGH, FILE_CONTRIB_HLL_PRECISION,
};
pub use tc::{CommitData, CommitSummary, RenamePair, TickData};
