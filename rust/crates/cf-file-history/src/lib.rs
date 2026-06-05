//! File history analyzer (`history/file-history`).
//!
//! Ports `internal/analyzers/file_history` (Go). The analyzer maps each file
//! path to the list of commits which touch that file together with the mapping
//! from involved developers to their line statistics. From that raw per-file
//! history it derives churn, contributor, hotspot, aggregate and file-composition
//! metrics.
//!
//! # Byte-identity
//!
//! All machine-format report serialization is routed through [`cf_gojson`]
//! rather than raw serde, per `specs/rust-rewrite/DESIGN.md` section 2. The pure
//! metric computation in [`metrics`] reproduces the Go behavior exactly
//! (including the descending churn-score and risk-then-commit-count sort orders);
//! the byte rendering of [`metrics::ComputedMetrics`] to a Go
//! `encoding/json`-compatible value lives in [`report`].
//!
//! # Nondeterminism note
//!
//! Per the Go source and DESIGN.md section 2.8, the `hashes` slice of a
//! [`metrics::FileHistory`] has Go-side nondeterministic order (it is appended
//! during a map-iteration-order traversal). Only its **length** (commit count)
//! feeds the metrics, so the derived metrics are deterministic; any report path
//! that emits raw hashes must be canonicalized (sorted) on both sides before
//! diffing.
//!
//! # Scope of this crate
//!
//! The fully self-contained, byte-identity-critical pieces are ported here: the
//! file classifier scaffold ([`classify`]), the per-commit transport payload
//! types ([`tc`]), the metric computation ([`metrics`]) and its Go-JSON
//! rendering ([`report`]). The streaming framework integration (the
//! `Aggregator`/`SpillStore`, checkpointing, hibernation and the git2
//! commit-tree traversal that filters deleted files) depends on framework crates
//! that are not yet stable in the Rust tree and is tracked as a roadmap item; the
//! corresponding interfaces are sketched in [`framework`].

pub mod classify;
pub mod framework;
pub mod metrics;
pub mod report;
pub mod tc;

pub use classify::{all_categories, Category, Classifier, ALL_CATEGORIES};
pub use metrics::{
    compute_aggregate_with_options, compute_all_metrics, compute_all_metrics_with_options,
    compute_composition, compute_file_churn, compute_file_contributors,
    compute_hotspots_with_options, AggregateData, ComputedMetrics, CompositionData,
    CompositionTimeSeriesEntry, ContributorEntry, FileChurnData, FileContributorData, FileHistory,
    HotspotData, MetricOptions, ReportData, HOTSPOT_THRESHOLD_CRITICAL, HOTSPOT_THRESHOLD_HIGH,
    HOTSPOT_THRESHOLD_MEDIUM,
};
pub use report::{computed_metrics_to_go, to_compact_json, to_compact_json_string};
pub use tc::{CategoryCounts, LineStats};

/// The analyzer identifier reported by [`metrics::ComputedMetrics::analyzer_name`].
pub const ANALYZER_NAME: &str = "file_history";

/// Stable analyzer descriptor identifier (`Descriptor.ID` in Go).
pub const DESCRIPTOR_ID: &str = "history/file-history";

/// The CLI flag for the analyzer (`Flag()` in Go).
pub const FLAG: &str = "file-history";

/// The human-readable analyzer name (`Name()` in Go).
pub const NAME: &str = "FileHistoryAnalysis";

/// The analyzer description (`Descriptor.Description` in Go).
pub const DESCRIPTION: &str = "Each file path is mapped to the list of commits which touch that file \
and the mapping from involved developers to the corresponding line statistics.";
