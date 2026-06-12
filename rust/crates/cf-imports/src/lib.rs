//! Import/dependency analysis.
//!
//! Two related analyzers:
//!
//! * [`analyzer::Analyzer`] — **static** analysis that produces the `imports`
//!   report key (the deduplicated import identifiers in a parsed file/tree) plus
//!   a `count`, along with the derived [`metrics::ComputedMetrics`] used for
//!   json/yaml/bin output.
//! * [`history::HistoryAnalyzer`] — **historical** analysis
//!   (`ImportsPerDeveloper`, stable id `history/imports`) that tracks import
//!   usage per developer, language, and tick in a 4-level
//!   [`history::ImportsMap`] which merges **additively**.
//!
//! Supporting modules: [`metrics`] (classification + metric computation),
//! [`aggregator`] (cross-file static aggregation), [`store`] (history persistence
//! records), and [`report_section`] (terminal section data).
//!
//! # Compatibility and serialization
//!
//! Every machine-format report (json, yaml, ndjson, timeseries, compact, bin)
//! is a frozen output contract, pinned byte-for-byte against the reference
//! implementation by the differential gate in `rust/tests/compat`. All such
//! output routes through the report-format encoders `cf-gojson` / `cf-goyaml`
//! and the CFB1 `bin` envelope from `cf-reportutil` — never raw serde. The
//! canonical [`node::Node`] model belongs to `cf-uast-node` and the framework
//! `Analyzer` trait / report type to `cf-framework` / `cf-analyze`.
//!
//! To remain self-contained and verifiable, this crate carries minimal local
//! shims — [`node`] (the UAST node subset the analyzer reads) and [`report`]
//! (the dynamic [`report::ReportValue`] model plus a deterministic compact-JSON
//! / CFB1 encoder). The analyzer logic depends only on the documented shim
//! contracts.

pub mod aggregator;
pub mod analyzer;
pub mod history;
pub mod metrics;
pub mod node;
pub mod report;
pub mod report_section;
pub mod store;

// Re-exports for the primary public surface.
pub use aggregator::Aggregator;
pub use analyzer::Analyzer;
pub use history::{CommitSummary, HistoryAnalyzer, ImportEntry, ImportsMap, TickData};
pub use metrics::{
    compute_all_metrics, AggregateData, ComputedMetrics, ImportCategoryData, ImportData,
    ImportDependencyData, ReportData,
};
pub use node::Node;
pub use report::ReportValue;
pub use report_section::ReportSection;
pub use store::{ImportUsageRecord, KIND_IMPORT_USAGE};
