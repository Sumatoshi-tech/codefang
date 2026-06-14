//! `--format plot` history-phase orchestration — the Rust analogue of the reference implementation's
//! plot pipeline (the reference `executePlotPipeline` → `FinalizeToStore` →
//! `enrichAnomalyFromStore` → `renderFromStore` = the reference `runRender`).
//!
//! Flow (per the reference `runRender` over the temp `FileReportStore`):
//!
//! 1. every selected history leaf writes its structured store kinds — here
//!    each leaf's section data is computed directly from the analyzer crates'
//!    metric surfaces (the same values the byte-verified json/yaml/bin reports
//!    encode), so no store round-trip is needed;
//! 2. `StorePlotSectionsFor(id)` builds the per-analyzer sections;
//!    `RenderAnalyzerPage(safeID, id, sections)` writes `<flag>.html` (the
//!    store id is the analyzer `Flag()`, slash-free, so `safeID == id`) and
//!    collects its [`PageMeta`] (`{ID: flag, Title: flag}`);
//! 3. `RenderIndex(pages)` writes `index.html`;
//! 4. `writeRenderReportJSON` writes `report.json` —
//!    `{"analyzer_ids": [...], "pages": [...]}`, two-space-indented
//!    `encoding/json` with the trailing newline, 0o640, atomic temp+rename.
//!    In a MIXED static+history run this overwrites the static phase's
//!    `report.json` (the reference implementation's history phase runs second), and `rebuildPlotIndex`
//!    then rescans the directory for the unified, title-sorted index.
//!
//! # Ordering
//!
//! The reference implementation's `analyzer_ids` order is the store-manifest write order — the pipeline
//! leaf order, which is MAP-RANDOM in the reference implementation (verified: three star runs produced
//! three different orders). The deterministic Rust stand-in is the separate-
//! phase emit order ([`crate::handlers::HISTORY_PHASE_EMIT_ORDER`]); the
//! oracle measures the reference implementation's variance and compares those files structurally.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use cf_anomaly::enrich::ExtractedSeries;
use cf_gojson::{Encoder, GoMap, GoValue, MapOrigin};
use cf_plotpage::{MultiPageRenderer, PageMeta, Section, Theme};

use crate::handlers::plot_sections::{
    history_anomaly, history_burndown, history_couples, history_devs, history_file_history,
    history_imports, history_quality, history_sentiment, history_shotness, history_typos,
};
use crate::handlers::{burndown_ndjson, couples_run, history, shotness_run};
use crate::pipeline::RunContext;

/// Project title shown on every plot page.
const PLOT_PAGE_TITLE: &str = "Codefang";

/// report.json file mode.
#[cfg(unix)]
const REPORT_JSON_MODE: u32 = 0o640;

/// One plot-capable history analyzer: registry id, store id (the analyzer
/// `Flag()` — page filename and title), and its section builder. The builder
/// receives the full selected-flag set so the anomaly leaf can reproduce the
/// cross-analyzer store enrichment.
pub struct HistoryPlotAnalyzer {
    /// Full registry id (`history/devs`).
    pub id: &'static str,
    /// Store id / page name (the analyzer `Flag()`, e.g. `devs`,
    /// `imports-per-dev`).
    pub flag: &'static str,
    /// Builds the page sections; `None` mirrors a reference section-renderer error
    /// (analyzer skipped from the page set but kept in `analyzer_ids`).
    pub sections: fn(&RunContext, &[&'static str]) -> Option<Vec<Section>>,
}

fn quality_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = history::quality_metrics(ctx.matches)?;
    Some(history_quality::sections(&metrics))
}

fn sentiment_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = history::sentiment_metrics(ctx.matches)?;
    Some(history_sentiment::sections(&metrics))
}

fn shotness_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let report = shotness_run::shotness_run_report_data(ctx.matches)?;
    Some(history_shotness::sections(&report.nodes, &report.counters))
}

fn couples_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let data = couples_run::couples_plot_data(ctx.matches)?;
    Some(history_couples::sections(&data.file_coupling, &data.dev_names, &data.ownership))
}

fn imports_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let usage = history::imports_run_usage_counts(ctx.matches)?;
    Some(history_imports::sections(&usage))
}

fn typos_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let report = history::typos_report_data(ctx.matches)?;
    let file_typos = cf_typos::metrics::compute_file_typos(&report);
    Some(history_typos::sections(&file_typos))
}

fn anomaly_sections(ctx: &RunContext, selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = if ctx.head() {
        history::anomaly_head_report(ctx.matches)?
    } else {
        history::anomaly_run_metrics(ctx.matches)?
    };

    // The reference `enrichAnomalyFromStore` → `runStoreEnrichment`: cross-analyzer
    // anomalies over the OTHER selected analyzers' store time series. Only
    // quality and sentiment register store extractors.
    let mut extracted: BTreeMap<String, ExtractedSeries> = BTreeMap::new();
    if selected.contains(&"quality") {
        if let Some(qm) = history::quality_metrics(ctx.matches) {
            if !qm.time_series.is_empty() {
                let ticks: Vec<i64> = qm.time_series.iter().map(|e| e.tick).collect();
                let dim = |f: fn(&cf_quality::TickStats) -> f64| -> Vec<f64> {
                    qm.time_series.iter().map(|e| f(&e.stats)).collect()
                };
                let dimensions: BTreeMap<String, Vec<f64>> = [
                    ("complexity_median", dim(|s| s.complexity_median)),
                    ("complexity_p95", dim(|s| s.complexity_p95)),
                    ("halstead_vol_median", dim(|s| s.halstead_vol_median)),
                    ("delivered_bugs_sum", dim(|s| s.delivered_bugs_sum)),
                    ("comment_score_min", dim(|s| s.comment_score_min)),
                    ("cohesion_min", dim(|s| s.cohesion_min)),
                ]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
                extracted.insert("quality".to_string(), ExtractedSeries { ticks, dimensions });
            }
        }
    }
    if selected.contains(&"sentiment") {
        if let Some(sm) = history::sentiment_metrics(ctx.matches) {
            if let Some((ticks, dims)) = cf_sentiment::store::extract_store_time_series(&sm.time_series)
            {
                extracted.insert(
                    "sentiment".to_string(),
                    ExtractedSeries {
                        ticks,
                        dimensions: dims.into_iter().collect(),
                    },
                );
            }
        }
    }

    let (_external_anomalies, external_summaries) = cf_anomaly::enrich::run_store_enrichment(
        &extracted,
        usize::try_from(metrics.aggregate.window_size).unwrap_or(0),
        f64::from(metrics.aggregate.threshold),
    );

    Some(history_anomaly::sections(&metrics, &external_summaries))
}

fn burndown_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    if ctx.head() {
        // Head pipeline: one TC at tick 0 — the global dense history is a
        // single `[total_current_lines]` sample; endTime is the HEAD commit's
        // committer time (the only TC timestamp).
        let metrics = history::burndown_head_metrics(ctx.matches)?;
        let repo = cf_gitlib::Repository::open(&ctx.path).ok()?;
        let head = repo.head().ok()?;
        let commit = repo.lookup_commit(head).ok()?;
        let end_time_ns = commit.committer().when.seconds().saturating_mul(1_000_000_000);
        let chart_data = history_burndown::ChartData {
            global_history: vec![vec![metrics.aggregate.total_current_lines]],
            sampling: 30,
            granularity: 30,
            tick_size_ns: 24 * 3_600_000_000_000,
            end_time_ns,
            project_name: burndown_ndjson::repo_base_name(&ctx.path),
        };
        return Some(history_burndown::sections(&chart_data, &metrics));
    }

    let agg = burndown_ndjson::burndown_run_aggregate(ctx.matches)?;
    // The reference `buildChartData`: clamp negative dense values to zero.
    let mut dense = agg.global_dense.clone();
    for row in &mut dense {
        for v in row.iter_mut() {
            if *v < 0 {
                *v = 0;
            }
        }
    }
    let chart_data = history_burndown::ChartData {
        global_history: dense,
        sampling: agg.sampling,
        granularity: agg.granularity,
        tick_size_ns: agg.tick_size_ns,
        end_time_ns: agg.end_time_ns,
        project_name: agg.project_name.clone(),
    };
    Some(history_burndown::sections(&chart_data, &agg.metrics))
}

fn devs_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    // `--head` WITHOUT first-parent: the sequential TreeDiff has no
    // predecessor for the lone non-root HEAD commit, so the devs leaf consumes
    // an empty TC — the closed-form empty metrics (`devs_head_metrics`).
    // WITH first-parent (forced whenever burndown is co-selected, e.g. the `*`
    // selection), TreeDiff diffs HEAD against `parent(0)` and the full
    // walk-path math applies — `devs_run_metrics`' walk already handles the
    // single-HEAD window (verified against the live reference plot: head+first-parent
    // carries the HEAD diff's languages and DMP line stats; head alone is
    // empty).
    let metrics = if ctx.head() && !crate::handlers::effective_first_parent(ctx.matches) {
        history::devs_head_metrics(ctx.matches)?
    } else {
        history::devs_run_metrics(ctx.matches)?
    };
    Some(history_devs::sections(&metrics))
}

fn file_history_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = history::file_history_run_metrics(ctx.matches)?;
    Some(history_file_history::sections(&metrics.file_churn, &metrics.composition_ts))
}

/// The plot-capable history analyzers in the separate-phase emit order
/// (the reference `pl.Leaves` order is map-random; see the module docs).
pub const HISTORY_PLOT_ANALYZERS: &[HistoryPlotAnalyzer] = &[
    HistoryPlotAnalyzer {
        id: "history/quality",
        flag: "quality",
        sections: quality_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/sentiment",
        flag: "sentiment",
        sections: sentiment_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/shotness",
        flag: "shotness",
        sections: shotness_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/couples",
        flag: "couples",
        sections: couples_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/imports",
        flag: "imports-per-dev",
        sections: imports_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/typos",
        flag: "typos",
        sections: typos_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/anomaly",
        flag: "anomaly",
        sections: anomaly_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/burndown",
        flag: "burndown",
        sections: burndown_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/devs",
        flag: "devs",
        sections: devs_sections,
    },
    HistoryPlotAnalyzer {
        id: "history/file-history",
        flag: "file-history",
        sections: file_history_sections,
    },
];

/// Runs the history plot phase for the selected registry ids and writes the
/// page set + `report.json` into `output_dir`. When `mixed` (the static plot
/// phase already wrote into the same directory), the final index is rebuilt
/// from the directory scan, title-sorted across both
/// phases. Returns `None` when any selected id has no plot entry (caller falls
/// through to the dispatch-blocked diagnostic), `Some(0)` on success,
/// `Some(1)` on an I/O failure.
#[must_use]
pub fn run_history_plot(
    ctx: &RunContext,
    history_ids: &[String],
    output_dir: &str,
    mixed: bool,
) -> Option<i32> {
    // Resolve the selection against the registry, keeping the canonical emit
    // order (the reference implementation's leaf order is map-random; the oracle measures the variance).
    let selected: Vec<&HistoryPlotAnalyzer> = HISTORY_PLOT_ANALYZERS
        .iter()
        .filter(|entry| history_ids.iter().any(|id| id == entry.id))
        .collect();
    if selected.len() != history_ids.len() || selected.is_empty() {
        return None;
    }
    let selected_flags: Vec<&'static str> = selected.iter().map(|e| e.flag).collect();

    match render_history_plot(ctx, &selected, &selected_flags, output_dir, mixed) {
        Ok(()) => Some(0),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(1)
        }
    }
}

/// Renders pages + index + report.json (the reference `runRender` [+ `rebuildPlotIndex`
/// for mixed runs]).
fn render_history_plot(
    ctx: &RunContext,
    selected: &[&HistoryPlotAnalyzer],
    selected_flags: &[&'static str],
    output_dir: &str,
    mixed: bool,
) -> io::Result<()> {
    create_output_dir(output_dir)?;

    let renderer = MultiPageRenderer {
        output_dir: output_dir.to_string(),
        title: PLOT_PAGE_TITLE.to_string(),
        theme: Theme::Dark,
    };

    // Pre-compute the ONE shared UAST walk for the co-selected heavy history
    // analyzers (imports/quality/sentiment/shotness/typos) — one task — then
    // COMPUTE the per-analyzer sections concurrently (independent walks /
    // memoized shared-walk reads) and render/write the pages in the existing
    // deterministic emit order below.
    crate::handlers::uast_walk::prewarm(ctx.matches);
    let all_sections: Vec<Option<Vec<Section>>> = crate::handlers::run_concurrent(
        selected.len(),
        crate::handlers::ANALYZER_CONCURRENCY,
        |i| (selected[i].sections)(ctx, selected_flags),
    );

    let mut pages: Vec<PageMeta> = Vec::new();
    for (entry, sections) in selected.iter().zip(all_sections) {
        // The reference `renderOneAnalyzer`: a section-build error skips the page (the
        // analyzer stays in analyzer_ids); empty sections still render a page.
        let Some(sections) = sections else {
            continue;
        };
        renderer.render_analyzer_page(entry.flag, entry.flag, sections)?;
        pages.push(PageMeta {
            id: entry.flag.to_string(),
            title: entry.flag.to_string(),
            description: String::new(),
        });
    }

    renderer.render_index(&pages)?;
    write_render_report_json(output_dir, selected_flags, &pages)?;

    if mixed {
        // The reference `rebuildPlotIndex`: rescan the output dir and regenerate the
        // unified title-sorted index across both phases.
        renderer.rebuild_index()?;
    }

    Ok(())
}

/// The reference `os.MkdirAll(outputDir, 0o750)` (reference `renderDirPerm`).
fn create_output_dir(output_dir: &str) -> io::Result<()> {
    let existed = Path::new(output_dir).is_dir();
    fs::create_dir_all(output_dir)?;
    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(output_dir, fs::Permissions::from_mode(0o750))?;
    }
    #[cfg(not(unix))]
    let _ = existed;
    Ok(())
}

/// The reference `writeRenderReportJSON`: `{"analyzer_ids": [...], "pages":
/// [{"ID", "Title", "Description"}...]}` — two-space indent, trailing newline,
/// atomic 0o640 write. `PageMeta` has no json tags, so the reference field names are
/// capitalized.
fn write_render_report_json(
    output_dir: &str,
    analyzer_ids: &[&'static str],
    pages: &[PageMeta],
) -> io::Result<()> {
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "analyzer_ids",
        GoValue::Array(analyzer_ids.iter().map(|id| GoValue::Str((*id).to_string())).collect()),
    );
    let page_values: Vec<GoValue> = pages
        .iter()
        .map(|p| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("ID", GoValue::Str(p.id.clone()));
            m.push("Title", GoValue::Str(p.title.clone()));
            m.push("Description", GoValue::Str(p.description.clone()));
            GoValue::Map(m)
        })
        .collect();
    root.push("pages", GoValue::Array(page_values));

    let bytes = Encoder::indented("  ")
        .with_trailing_newline(true)
        .encode(&GoValue::Map(root));

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
