//! `--format plot` static-phase orchestration, plus the output-directory
//! helpers the history plot phase ([`super::history_plot`]) shares.
//!
//! Flow:
//!
//! 1. run the selected static analyzers and gather each one's AGGREGATED RAW
//!    report (the same map `report.json` carries);
//! 2. for each analyzer with a registered section renderer, render
//!    `<output>/<id-with-dashes>.html` (page title = the full analyzer id) and
//!    collect its [`PageMeta`];
//! 3. render `<output>/index.html` from the collected metas;
//! 4. write `<output>/report.json` — the results map keyed by the analyzer's
//!    short name ([`write_report_json_atomic`]).

use std::fs;
use std::io;
use std::path::Path;

use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_plotpage::{MultiPageRenderer, PageMeta, Theme};

use crate::handlers::plot_sections::{self, SectionsFn};
use crate::handlers::{
    static_clones, static_cohesion, static_comments, static_complexity, static_halstead,
    static_imports, static_json,
};
use crate::pipeline::RunContext;

/// Project title shown on every plot page.
const PLOT_PAGE_TITLE: &str = "Codefang";

/// Mode of an output directory a plot run creates.
#[cfg(unix)]
const PLOT_DIR_MODE: u32 = 0o750;

/// `report.json` file mode.
#[cfg(unix)]
const REPORT_JSON_MODE: u32 = 0o640;

/// One plot-capable static analyzer: its registry id, the short name keying
/// the `report.json` results map, the builder of its aggregated raw report,
/// and its optional section renderer. An analyzer with `sections: None`
/// renders no page but still has its report in `report.json`.
pub struct PlotAnalyzer {
    /// Full analyzer id (`static/complexity`).
    pub id: &'static str,
    /// Short report name (`complexity`) — the `report.json` map key.
    pub name: &'static str,
    /// Builds the aggregated raw `analyze.Report` value.
    pub raw_report: fn(&RunContext) -> Option<GoValue>,
    /// The registered section renderer.
    pub sections: Option<SectionsFn>,
}

fn clones_raw(ctx: &RunContext) -> Option<GoValue> {
    static_clones::clones_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn complexity_raw(ctx: &RunContext) -> Option<GoValue> {
    static_complexity::complexity_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn comments_raw(ctx: &RunContext) -> Option<GoValue> {
    static_comments::comments_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn halstead_raw(ctx: &RunContext) -> Option<GoValue> {
    static_halstead::halstead_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn cohesion_raw(ctx: &RunContext) -> Option<GoValue> {
    static_cohesion::cohesion_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn imports_raw(ctx: &RunContext) -> Option<GoValue> {
    static_imports::imports_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

fn composition_raw(ctx: &RunContext) -> Option<GoValue> {
    static_json::composition_raw_report_value(&ctx.path, &super::static_filter(ctx).ok()?)
}

/// The plot-capable static analyzers in registry order: clones, complexity,
/// comments, halstead, cohesion, imports, then composition (no page;
/// `report.json` only).
pub const PLOT_ANALYZERS: &[PlotAnalyzer] = &[
    PlotAnalyzer {
        id: "static/clones",
        name: "clones",
        raw_report: clones_raw,
        sections: Some(plot_sections::clones::sections),
    },
    PlotAnalyzer {
        id: "static/complexity",
        name: "complexity",
        raw_report: complexity_raw,
        sections: Some(plot_sections::complexity::sections),
    },
    PlotAnalyzer {
        id: "static/comments",
        name: "comments",
        raw_report: comments_raw,
        sections: Some(plot_sections::comments::sections),
    },
    PlotAnalyzer {
        id: "static/halstead",
        name: "halstead",
        raw_report: halstead_raw,
        sections: Some(plot_sections::halstead::sections),
    },
    PlotAnalyzer {
        id: "static/cohesion",
        name: "cohesion",
        raw_report: cohesion_raw,
        sections: Some(plot_sections::cohesion::sections),
    },
    PlotAnalyzer {
        id: "static/imports",
        name: "imports",
        raw_report: imports_raw,
        sections: Some(plot_sections::imports::sections),
    },
    PlotAnalyzer {
        id: "static/composition",
        name: "composition",
        raw_report: composition_raw,
        sections: None,
    },
];

/// Runs the static plot phase for the selected literal analyzer ids and writes
/// the page set + `report.json` into `output_dir`. Returns `None` when any
/// selected id has no plot entry yet (caller falls through to the
/// dispatch-blocked diagnostic), `Some(0)` on success, `Some(1)` on an I/O
/// failure (the error is printed as `Error: <cause>`).
#[must_use]
pub fn run_static_plot(ctx: &RunContext, static_ids: &[String], output_dir: &str) -> Option<i32> {
    // Pages (and the index cards) follow the SELECTION order; a glob
    // selection arrives here already expanded in registry order.
    let mut selected: Vec<&PlotAnalyzer> = Vec::new();
    for id in static_ids {
        let entry = PLOT_ANALYZERS.iter().find(|entry| entry.id == id)?;
        selected.push(entry);
    }
    if selected.is_empty() {
        return None;
    }

    // The static analyzers are independent folder walks: COMPUTE their
    // reports concurrently and keep the selection order for rendering and
    // writing below.
    let reports: Vec<Option<GoValue>> = crate::handlers::run_concurrent(
        selected.len(),
        crate::handlers::ANALYZER_CONCURRENCY,
        |i| (selected[i].raw_report)(ctx),
    );
    let mut results: Vec<(&'static str, GoValue)> = Vec::new();
    for (entry, report) in selected.iter().zip(reports) {
        results.push((entry.name, report?));
    }

    match render_plot_output(&selected, &results, output_dir) {
        Ok(()) => Some(0),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(1)
        }
    }
}

/// Renders pages + index + report.json.
fn render_plot_output(
    selected: &[&PlotAnalyzer],
    results: &[(&'static str, GoValue)],
    output_dir: &str,
) -> io::Result<()> {
    create_output_dir(output_dir)?;
    let renderer = page_renderer(output_dir);

    // Pages only for analyzers with a section renderer whose sections build
    // (an analyzer whose sections fail keeps its report.json entry).
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
    let mut map = GoMap::new(MapOrigin::Map);
    for (name, report) in results {
        map.push(*name, report.clone());
    }
    write_report_json_atomic(output_dir, &GoValue::Map(map))
}

/// The dark-themed multi-page renderer writing into `output_dir`.
pub(crate) fn page_renderer(output_dir: &str) -> MultiPageRenderer {
    MultiPageRenderer {
        output_dir: output_dir.to_string(),
        title: PLOT_PAGE_TITLE.to_string(),
        theme: Theme::Dark,
    }
}

/// Creates `output_dir` (and its parents); a directory this run creates gets
/// mode 0o750, a pre-existing one keeps its mode.
pub(crate) fn create_output_dir(output_dir: &str) -> io::Result<()> {
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

/// Writes `value` to `<output_dir>/report.json` as two-space-indented JSON
/// with a trailing newline, atomically (temp file + rename), mode 0o640.
pub(crate) fn write_report_json_atomic(output_dir: &str, value: &GoValue) -> io::Result<()> {
    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode(value);

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
