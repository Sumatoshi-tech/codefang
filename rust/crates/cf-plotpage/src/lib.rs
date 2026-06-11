//! `cf-plotpage` — the `--format plot` HTML-page renderer, the Rust port of Go
//! `internal/analyzers/common/plotpage` (+ the go-echarts v2.6.7 subset it
//! renders through).
//!
//! Pages must be byte-identical to the Go binary's output modulo the chart
//! element IDs: go-echarts draws a random 12-char `[A-Za-z]` ID per chart per
//! run (`util.GenerateUniqueID`), which is the only run-to-run nondeterminism
//! in a Go plot page. The Rust side replaces that with the deterministic
//! per-page [`echarts::ChartIdGen`] sequence, so two Rust runs are
//! byte-identical end to end.
//!
//! Layout mirrors the Go package:
//!
//! * [`theme`] — theme.go (theme configs + chart palettes);
//! * [`echarts`] — the go-echarts option model: per-struct serializers that
//!   reproduce `BaseConfiguration.JSONNotEscaped` byte-for-byte (top-level map
//!   keys byte-sorted, nested struct keys in Go declaration order, compact
//!   `encoding/json` with `SetEscapeHTML(false)`), plus the extracted chart
//!   element/script snippet (`plotpage.extractChartContent` form);
//! * [`chart_opts`] — chart_opts.go (themed option presets);
//! * [`builders`] — builders.go (`BuildBarChart` / `BuildLineChart` /
//!   `BuildPieChart`);
//! * [`components`] — components.go (tabs/card/badge/text/grid/stat/alert/
//!   table) + the [`components::Renderable`] trait;
//! * [`templates`] — templates.go + templates/*.html, carried as the
//!   POST-`html/template` literal bytes with escaped data slots;
//! * [`page`] — plotpage.go (`Page` + `HTMLRenderer` + `Section`);
//! * [`multipage`] — multipage.go (`MultiPageRenderer` + `PageMeta`).
//!
//! All JSON routes through `cf-gojson` (never serde): the chart option JSON is
//! Go `encoding/json` output and byte-compared against the live Go binary.

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
