//! `cf-couples` — file/developer co-change coupling analysis.
//!
//! Implements the math of the `history/couples` analyzer. From the per-commit
//! payloads the history walk produces ([`CommitData`]) it derives two
//! co-occurrence ("coupling") matrices and per-file ownership:
//!
//! - **File coupling** — the number of commits in which a pair of files both
//!   changed; the diagonal is a file's own change count
//!   ([`matrix::compute_files_matrix`], [`metrics::compute_file_coupling`]).
//! - **Developer coupling** — `sum over files of min(changes_i, changes_j)`
//!   for a pair of developers; the diagonal is a developer's own file-change
//!   count ([`matrix::compute_people_matrix`],
//!   [`metrics::compute_developer_coupling`]).
//! - **File ownership** — per-file line count and contributor cardinality
//!   ([`metrics::compute_file_ownership`]).
//! - **Aggregate** summary statistics ([`metrics::compute_aggregate`]).
//!
//! The coupling-strength formula is code-maat's `co_changes / avg(revs_a,
//! revs_b)` capped at `1.0`, where `revs` is the diagonal (self-change) count.
//!
//! ## Renames
//!
//! A rename carries the old path's accumulated row, column and ownership
//! onto the new path at the commit it happens
//! ([`aggregator::Aggregator::add`]), so a moved file keeps one identity.
//!
//! ## Determinism
//!
//! All index assignment flows from byte-sorted name lists, so the matrices,
//! sequences and report are a pure function of the walked payloads. Report
//! bytes are produced via [`cf_gojson`] in [`report`]; this crate never uses
//! `serde_json` for output.

#![forbid(unsafe_code)]

pub mod aggregator;
pub mod matrix;
pub mod metrics;
pub mod report;
pub mod tc;

/// Maximum number of files a commit may touch and still feed the coupling
/// matrix. A larger changeset is a bulk operation (vendor update, mass
/// rename, formatting) whose co-changes are noise, so it contributes no
/// coupling context — its author still owns every file it touched.
pub const COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE: usize = 1000;

/// Splits an identity string `"name|email"` into its components.
/// Thin re-export over [`cf_identity`] so the crate's metric code can call
/// `crate::split_identity`.
///
/// ```
/// use cf_couples::split_identity;
///
/// let (name, email) = split_identity("Ada Lovelace|ada@example.com");
/// assert_eq!(name, "Ada Lovelace");
/// assert_eq!(email, "ada@example.com");
/// ```
#[must_use]
pub fn split_identity(s: &str) -> (String, String) {
    cf_identity::split_identity(s)
}

pub use metrics::{
    bucket_ownership, bucket_ownership_with_thresholds, compute_aggregate, compute_all_metrics,
    compute_all_metrics_with_options, compute_developer_coupling, compute_file_coupling,
    compute_file_ownership, AggregateData, ComputedMetrics, DeveloperCouplingData,
    FileCouplingData, FileOwnershipData, MetricOptions, OwnershipBucket, ReportData,
    COUPLING_THRESHOLD_HIGH, FILE_CONTRIB_HLL_PRECISION,
};
pub use tc::{CommitData, RenamePair};
