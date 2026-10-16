//! File history analyzer (`history/file-history`) — the metric math.
//!
//! The history walk (in `cf-commands`) maps each file path to the commits
//! that touched it, following renames, together with the per-developer line
//! statistics, and classifies every changed file into a [`Category`]. From
//! that raw per-file history this crate derives churn, contributor, hotspot
//! and aggregate metrics ([`metrics`]) and the file-composition breakdown
//! (distinct changed files per category, overall and per tick), and renders
//! them as the report value ([`report`]) through [`cf_gojson`] — never raw
//! serde.
//!
//! Only the **length** of a [`metrics::FileHistory`]'s commit list feeds the
//! metrics, so the derived numbers do not depend on the order the hashes
//! were appended in.

// Deliberate casts (usize -> i64 lengths, i64 -> f64 averages) are part of
// the report contract; the lossy-cast lints would only add noise.
#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

pub mod category;
pub mod metrics;
pub mod report;
pub mod tc;

pub use category::{Category, ALL_CATEGORIES};
pub use metrics::{
    compute_aggregate_with_options, compute_all_metrics, compute_all_metrics_with_options,
    compute_composition, compute_file_churn, compute_file_contributors,
    compute_hotspots_with_options, AggregateData, CompositionData, CompositionInput,
    CompositionTimeSeriesEntry, ComputedMetrics, ContributorEntry, FileChurnData,
    FileContributorData, FileHistory, HotspotData, MetricOptions, ReportData, TickBounds,
    HOTSPOT_THRESHOLD_CRITICAL, HOTSPOT_THRESHOLD_HIGH, HOTSPOT_THRESHOLD_MEDIUM,
};
pub use report::computed_metrics_to_go;
pub use tc::{CategoryCounts, LineStats};
