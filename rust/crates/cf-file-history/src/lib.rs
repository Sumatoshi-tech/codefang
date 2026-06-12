//! File history analyzer (`history/file-history`).
//!
//! Maps each file path to the list of commits which touch that file together
//! with the mapping from involved developers to their line statistics. From
//! that raw per-file history it derives churn, contributor, hotspot, aggregate
//! and file-composition metrics.
//!
//! # Compatibility
//!
//! Output bytes are pinned against the reference implementation by
//! `rust/tests/compat`. All machine-format report serialization is routed
//! through [`cf_gojson`] rather than raw serde. The pure metric computation in
//! [`metrics`] preserves the contract's sort orders (descending churn score;
//! risk then commit count); the byte rendering of [`metrics::ComputedMetrics`]
//! lives in [`report`].
//!
//! # Nondeterminism note
//!
//! The `hashes` slice of a [`metrics::FileHistory`] has nondeterministic order
//! in the reference implementation (it is appended during a hash-map
//! traversal). Only its **length** (commit count) feeds the metrics, so the
//! derived metrics are deterministic; any report path that emits raw hashes
//! must be canonicalized (sorted) on both sides before diffing.
//!
//! # Scope of this crate
//!
//! The fully self-contained, byte-contract-critical pieces live here: the file
//! classifier scaffold ([`classify`]), the per-commit transport payload types
//! ([`tc`]), the metric computation ([`metrics`]) and its report rendering
//! ([`report`]). The streaming framework integration (aggregator, spill store,
//! checkpointing, hibernation and the commit-tree traversal that filters
//! deleted files) depends on framework crates that are not yet stable in this
//! tree and is tracked as a roadmap item; the corresponding interfaces are
//! sketched in [`framework`].

// Deliberate parity casts (usize -> i64 lengths, i64 -> f64 averages) are part
// of the report contract; the lossy-cast lints would only add noise.
#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

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

/// Stable analyzer descriptor identifier.
pub const DESCRIPTOR_ID: &str = "history/file-history";

/// The CLI flag for the analyzer.
pub const FLAG: &str = "file-history";

/// The human-readable analyzer name.
pub const NAME: &str = "FileHistoryAnalysis";

/// The analyzer description shown in CLI help.
pub const DESCRIPTION: &str = "Each file path is mapped to the list of commits which touch that file \
and the mapping from involved developers to the corresponding line statistics.";
