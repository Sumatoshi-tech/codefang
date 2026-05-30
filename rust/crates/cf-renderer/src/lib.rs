//! Report renderer dispatch for analyzers.
//!
//! This crate is a faithful port of the Go package
//! `internal/analyzers/common/renderer`. It is shared by the analyzers
//! (`clones`, `cohesion`, `comments`, `complexity`, `halstead`, `imports`) and
//! the CLI, and provides:
//!
//! - **Structured JSON model** ([`json`]) — the [`json::JsonReport`] /
//!   [`json::JsonSection`] tree with **score-last** field ordering and the
//!   **initialized-empty `[]` vs `omitempty`** nuance, serialized through a
//!   Go-`encoding/json`-byte-compatible encoder.
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
//! Per DESIGN.md §1.1/§2, machine-format report bytes must match Go's
//! `encoding/json`. All JSON serialization here routes through the
//! [`gocompat`] encoder (map-key byte-sorting, HTML escaping on, Go float
//! formatting, score-last via declaration-ordered objects), never
//! `serde_json`. [`gocompat`] mirrors the design's tier-0 `cf-gojson` crate and
//! is replaced by it once that crate lands.
//!
//! # Not-yet-ported dependencies
//!
//! The Go renderer depends on `internal/analyzers/analyze` and
//! `internal/analyzers/common/terminal`, whose Rust crates (`cf-analyze`,
//! `cf-terminal`) are still scaffolds. Their renderer-facing surface is
//! reproduced in the [`analyze`] and [`terminal`] modules and will be replaced
//! by path dependencies once those crates are ported (see the crate `Cargo.toml`).

#![forbid(unsafe_code)]

pub mod analyze;
pub mod gocompat;
pub mod json;
pub mod metrics_output;
pub mod section_renderer;
pub mod static_renderer;
pub mod summary;
pub mod terminal;

// Flatten the most-used items to the crate root, mirroring the flat Go package.
pub use json::{
    section_to_json, section_to_json_file_entry, sections_to_json, JsonDistribution, JsonFileEntry,
    JsonIssue, JsonMetric, JsonReport, JsonSection,
};
pub use metrics_output::{render_metrics_json, render_metrics_yaml, MetricsOutput, NilMetricsOutput};
pub use section_renderer::{color_for_severity, SectionRenderer};
pub use static_renderer::DefaultStaticRenderer;
pub use summary::{ExecutiveSummary, MIN_SECTIONS_FOR_SUMMARY};
