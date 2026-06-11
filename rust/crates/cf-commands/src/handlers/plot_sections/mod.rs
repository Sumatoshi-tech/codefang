//! Per-analyzer plot section builders — the Rust analogue of Go's
//! `analyze.RegisterPlotSections` registry (conversion.go:257) populated by
//! each analyzer's `RegisterPlotSections()` (run.go:213).
//!
//! # Adding an analyzer's sections
//!
//! 1. Create `plot_sections/<analyzer>.rs` with
//!    `pub fn sections(report: &GoValue) -> Option<Vec<Section>>` — the port
//!    of that analyzer's Go `plot.go` section renderer, consuming the
//!    analyzer's AGGREGATED RAW report value (the `analyze.Report` map, NOT
//!    the renderer JSON document).
//! 2. Declare the module below.
//! 3. Add ONE registration line to
//!    [`crate::handlers::plot::PLOT_ANALYZERS`] wiring the analyzer id, its
//!    short report name, its raw-report builder, and `Some(<module>::sections)`.

pub mod clones;
pub mod cohesion;
pub mod comments;
pub mod complexity;
pub mod halstead;
pub mod imports;

use cf_gojson::GoValue;
use cf_plotpage::Section;

/// The per-analyzer section renderer signature (Go `analyze.SectionRendererFunc`:
/// `func(report analyze.Report) ([]plotpage.Section, error)` — `None` is the
/// error case, which Go's page loop skips).
pub type SectionsFn = fn(&GoValue) -> Option<Vec<Section>>;
