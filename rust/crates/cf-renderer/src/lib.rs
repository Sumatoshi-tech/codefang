//! Report renderer dispatch for analyzers.
//!
//! Shared by the analyzers (`clones`, `cohesion`, `comments`, `complexity`,
//! `halstead`, `imports`) and the CLI. Provides:
//!
//! - **Structured JSON model** ([`json`]) — the [`json::JsonReport`] /
//!   [`json::JsonSection`] tree with **score-last** field ordering and the
//!   **initialized-empty `[]` vs `omitempty`** nuance, serialized through a
//!   report-format byte-compatible encoder.
//! - **Terminal section rendering** ([`section_renderer`]) — the
//!   [`section_renderer::SectionRenderer`] producing the header box, key
//!   metrics, distribution bars, and issue lists.
//! - **Executive summary** ([`summary`]) — [`summary::ExecutiveSummary`] and
//!   `SectionRenderer::render_summary`.
//! - **Metrics-first output pipeline** ([`metrics_output`]) — the
//!   [`metrics_output::MetricsOutput`] trait plus JSON/YAML render helpers.
//! - **Default static renderer** ([`static_renderer`]) — high-level
//!   text/compact/JSON entry points used by `run`.
//!
//! # Byte identity
//!
//! Machine-format report bytes are a frozen contract, pinned against the
//! reference implementation by `rust/tests/compat` (DESIGN.md §1.1/§2). All
//! JSON serialization here routes through the [`gocompat`] encoder (map-key
//! byte-sorting, HTML escaping on, report-contract float formatting, score-last
//! via declaration-ordered objects), never `serde_json`. [`gocompat`] mirrors
//! the `GoValue`/`Encoder` API shape of the shared `cf-gojson` crate.
//!
//! # Local dependency surfaces
//!
//! The renderer-facing subsets of the analysis model and the terminal helpers
//! live in the local [`analyze`] and [`terminal`] modules (see the crate
//! `Cargo.toml` for the consolidation plan with `cf-analyze`/`cf-terminal`).
//!
//! # Example: render sections to report-contract JSON
//!
//! Build report sections, convert them to the structured JSON model, and emit
//! the byte-exact JSON. The overall score averages the scored sections, and
//! `metrics`/`issues` always serialize as `[]`:
//!
//! ```
//! use cf_renderer::sections_to_json;
//! use cf_renderer::analyze::{BaseReportSection, ReportSection};
//!
//! let complexity = BaseReportSection {
//!     title: "COMPLEXITY".to_string(),
//!     message: "Good".to_string(),
//!     score_value: 0.8,
//! };
//! let comments = BaseReportSection {
//!     title: "COMMENTS".to_string(),
//!     message: "Fair".to_string(),
//!     score_value: 0.6,
//! };
//!
//! let sections: Vec<&dyn ReportSection> = vec![&complexity, &comments];
//! let json = sections_to_json(&sections).to_json();
//!
//! assert!(json.contains(r#""title":"COMPLEXITY""#));
//! assert!(json.contains(r#""metrics":[]"#));
//! // Overall score is the mean (0.7) and is emitted last.
//! assert!(json.ends_with(r#""overall_score":0.7}"#));
//! ```

#![forbid(unsafe_code)]

pub mod analyze;
pub mod gocompat;
pub mod json;
pub mod metrics_output;
pub mod section_renderer;
pub mod static_renderer;
pub mod summary;
pub mod terminal;

// Flatten the most-used items to the crate root.
pub use json::{
    section_to_json, section_to_json_file_entry, sections_to_json, JsonDistribution, JsonFileEntry,
    JsonIssue, JsonMetric, JsonReport, JsonSection,
};
pub use metrics_output::{render_metrics_json, render_metrics_yaml, MetricsOutput, NilMetricsOutput};
pub use section_renderer::{color_for_severity, SectionRenderer};
pub use static_renderer::DefaultStaticRenderer;
pub use summary::{ExecutiveSummary, MIN_SECTIONS_FOR_SUMMARY};
