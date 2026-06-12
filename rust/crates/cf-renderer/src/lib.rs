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
