//! `cf-plotpage` — the `--format plot` HTML-page renderer (with the
//! go-echarts v2.6.7 option-model subset it renders through).
//!
//! Pages must be byte-identical to the reference binary's output modulo the
//! chart element IDs: go-echarts draws a random 12-char `[A-Za-z]` ID per
//! chart per run (`util.GenerateUniqueID`), which is the only run-to-run
//! nondeterminism in a reference plot page. This crate replaces that with the
//! deterministic per-page [`echarts::ChartIdGen`] sequence, so two runs are
//! byte-identical end to end.
//!
//! Module layout:
//!
//! * [`theme`] — theme configs + chart palettes;
//! * [`echarts`] — the go-echarts option model: per-struct serializers that
//!   reproduce `BaseConfiguration.JSONNotEscaped` byte-for-byte (top-level map
//!   keys byte-sorted, nested struct keys in declaration order, compact JSON
//!   with HTML escaping off), plus the extracted chart element/script snippet;
//! * [`chart_opts`] — themed option presets;
//! * [`builders`] — `build_bar_chart` / `build_line_chart` /
//!   `build_pie_chart`;
//! * [`components`] — tabs/card/badge/text/grid/stat/alert/table + the
//!   [`components::Renderable`] trait;
//! * [`templates`] — the page-shell template literals, carried as the
//!   POST-`html/template` bytes with escaped data slots;
//! * [`page`] — `Page` + `HtmlRenderer` + `Section`;
//! * [`multipage`] — `MultiPageRenderer` + `PageMeta`.
//!
//! All JSON routes through `cf-gojson` (never serde): the chart option JSON
//! is report-contract output, byte-compared against the reference binary by
//! `rust/tests/compat`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod builders;
pub mod chart_opts;
pub mod components;
pub mod echarts;
pub mod multipage;
pub mod page;
pub mod templates;
pub mod theme;

pub use builders::{
    build_bar_chart, build_line_chart, build_pie_chart, BarSeries, LineSeries, SeriesValue,
};
pub use chart_opts::ChartOpts;
pub use components::{html_escape, html_escape_string, RawHtml, Renderable};
pub use echarts::{Chart, ChartIdGen, ChartKind};
pub use multipage::{MultiPageRenderer, PageMeta};
pub use page::{render_analyzer_page, Hint, HtmlRenderer, Page, Section, Style};
pub use theme::{get_chart_palette, get_theme_config, ChartPalette, Theme, ThemeConfig};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-plotpage";
