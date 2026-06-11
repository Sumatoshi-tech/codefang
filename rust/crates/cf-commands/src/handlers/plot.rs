//! `--format plot` static-phase orchestration — the Rust analogue of Go
//! `runStaticPlotAnalyzers` (run.go:953) → `StaticService.FormatPlotPages`
//! (static.go:992).
//!
//! Flow (per Go):
//!
//! 1. run the selected static analyzers and gather each one's AGGREGATED RAW
//!    report (the `analyze.Report` value — the same map `report.json` carries);
//! 2. `RenderPlotPages`: for each analyzer with a registered section renderer,
//!    render `<output>/<id-with-dashes>.html` (page title = the full analyzer
//!    id) and collect its [`PageMeta`];
//! 3. `RenderIndex`: render `<output>/index.html` from the collected metas;
//! 4. `writeReportJSON`: write `<output>/report.json` — the results map keyed
//!    by the analyzer's short name, two-space-indented `encoding/json` with
//!    the `Encoder.Encode` trailing newline, 0o640 file mode, atomic
//!    temp+rename (Go `storage.WriteAtomic`).

use std::fs;
use std::io;
use std::path::Path;

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_plotpage::{MultiPageRenderer, PageMeta, Theme};

use crate::handlers::plot_sections::{self, SectionsFn};
use crate::handlers::{static_complexity, static_path_policy};
use crate::pipeline::RunContext;

/// Project title shown on every plot page (Go `plotPageTitle`, static.go:912).
const PLOT_PAGE_TITLE: &str = "Codefang";

/// Output directory mode (Go `plotDirPerm`, static.go:921).
#[cfg(unix)]
const PLOT_DIR_MODE: u32 = 0o750;

/// report.json file mode (Go `reportJSONPerm`, static.go:987).
#[cfg(unix)]
const REPORT_JSON_MODE: u32 = 0o640;

/// One plot-capable static analyzer: its registry id, the short name keying
/// the `report.json` results map (Go analyzer `Name()`), the builder of its
/// aggregated raw report, and its optional section renderer. Analyzers whose
/// sections are not yet ported keep `sections: None` — Go renders no page for
/// them but still includes their report in `report.json`.
pub struct PlotAnalyzer {
    /// Full analyzer id (`static/complexity`).
    pub id: &'static str,
    /// Short report name (`complexity`) — the `report.json` map key.
    pub name: &'static str,
    /// Builds the aggregated raw `analyze.Report` value.
    pub raw_report: fn(&RunContext) -> Option<GoValue>,
    /// The registered section renderer (Go `PlotSectionsFor(id)`).
    pub sections: Option<SectionsFn>,
}

fn complexity_raw(ctx: &RunContext) -> Option<GoValue> {
    static_complexity::complexity_raw_report_value(&ctx.path, &static_path_policy(ctx))
}

/// The plot-capable static analyzers in registry order (Go
/// `defaultUASTAnalyzers ++ defaultRawFileAnalyzers`). One line per analyzer;
/// see `plot_sections/mod.rs` for the porting recipe.
pub const PLOT_ANALYZERS: &[PlotAnalyzer] = &[PlotAnalyzer {
    id: "static/complexity",
    name: "complexity",
    raw_report: complexity_raw,
    sections: Some(plot_sections::complexity::sections),
}];

/// Runs the static plot phase for the selected literal analyzer ids and writes
/// the page set + `report.json` into `output_dir`. Returns `None` when any
/// selected id has no plot entry yet (caller falls through to the
/// dispatch-blocked diagnostic), `Some(0)` on success, `Some(1)` on an I/O
/// failure (Go surfaces the render error through cobra, rc 1).
#[must_use]
pub fn run_static_plot(ctx: &RunContext, static_ids: &[String], output_dir: &str) -> Option<i32> {
    // Resolve every selected id to its plot entry (registry order).
    let mut selected: Vec<&PlotAnalyzer> = Vec::new();
    for entry in PLOT_ANALYZERS {
        if static_ids.iter().any(|id| id == entry.id) {
            selected.push(entry);
        }
    }
    if selected.is_empty() || selected.len() != static_ids.len() {
        return None;
    }

    // Run the analyzers (Go service.AnalyzeFolder over the shared folder walk).
    let mut results: Vec<(&'static str, GoValue)> = Vec::new();
    for entry in &selected {
        let report = (entry.raw_report)(ctx)?;
        results.push((entry.name, report));
    }

    match render_plot_output(&selected, &results, output_dir) {
        Ok(()) => Some(0),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(1)
        }
    }
}

/// Renders pages + index + report.json (Go `FormatPlotPages`).
fn render_plot_output(
    selected: &[&PlotAnalyzer],
    results: &[(&'static str, GoValue)],
    output_dir: &str,
) -> io::Result<()> {
    create_output_dir(output_dir)?;

    let renderer = MultiPageRenderer {
        output_dir: output_dir.to_string(),
        title: PLOT_PAGE_TITLE.to_string(),
        theme: Theme::Dark,
    };

    // RenderPlotPages: pages only for analyzers with a section renderer whose
    // sections build successfully (Go skips section errors silently).
    let mut pages: Vec<PageMeta> = Vec::new();
    for (entry, (_, report)) in selected.iter().zip(results) {
        let Some(section_fn) = entry.sections else {
            continue;
        };
        let Some(sections) = section_fn(report) else {
            continue;
        };
        let safe_id = entry.id.replace('/', "-");
        renderer.render_analyzer_page(&safe_id, entry.id, sections)?;
        pages.push(PageMeta {
            id: safe_id,
            title: entry.id.to_string(),
            description: String::new(),
        });
    }

    renderer.render_index(&pages)?;
    write_report_json(results, output_dir)
}

/// Go `os.MkdirAll(outputDir, 0o750)` — apply the Go mode to directories this
/// run creates; pre-existing directories keep their mode (as in Go).
fn create_output_dir(output_dir: &str) -> io::Result<()> {
    let existed = Path::new(output_dir).is_dir();
    fs::create_dir_all(output_dir)?;
    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output_dir, fs::Permissions::from_mode(PLOT_DIR_MODE))?;
    }
    #[cfg(not(unix))]
    let _ = existed;
    Ok(())
}

/// Go `writeReportJSON` (static.go:1017): the results map (analyzer short name
/// → raw report) as two-space-indented JSON + the `Encoder.Encode` trailing
/// newline, written atomically (temp file + rename) with mode 0o640.
fn write_report_json(results: &[(&'static str, GoValue)], output_dir: &str) -> io::Result<()> {
    let mut map = GoMap::new(MapOrigin::Map);
    for (name, report) in results {
        map.push(*name, report.clone());
    }
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode(&GoValue::Map(map));

    let final_path = Path::new(output_dir).join("report.json");
    let tmp_path = Path::new(output_dir).join(".report.json.tmp");
    fs::write(&tmp_path, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(REPORT_JSON_MODE))?;
    }
    fs::rename(&tmp_path, &final_path)
}
