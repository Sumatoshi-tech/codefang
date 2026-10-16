//! `--format plot` history-phase orchestration.
//!
//! Flow:
//!
//! 1. every selected history analyzer's page sections are computed from its
//!    typed metrics (the same values its json/yaml/bin reports encode) —
//!    concurrently, after the shared UAST walk is warmed;
//! 2. each analyzer with sections renders `<flag>.html` (the analyzer flag,
//!    e.g. `devs`, `imports-per-dev`, is the page id and title) and adds a
//!    [`PageMeta`] card;
//! 3. `index.html` lists the cards;
//! 4. `report.json` records `{"analyzer_ids": [...], "pages": [...]}`. In a
//!    MIXED static+history run it replaces the static phase's `report.json`,
//!    and the index is rebuilt from a directory scan so it lists both phases'
//!    pages, title-sorted.
//!
//! Pages render in the separate-phase emit order
//! ([`HISTORY_PLOT_ANALYZERS`]), so the output does not depend on the order
//! of the selection.

use std::collections::BTreeMap;
use std::io;

use cf_anomaly::enrich::ExtractedSeries;
use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_plotpage::{PageMeta, Section};

use crate::handlers::plot::{create_output_dir, page_renderer, write_report_json_atomic};
use crate::handlers::plot_sections::{
    history_anomaly, history_burndown, history_couples, history_devs, history_file_history,
    history_imports, history_quality, history_sentiment, history_shotness, history_typos,
};
use crate::handlers::{burndown_ndjson, couples_run, history, shotness_run};
use crate::pipeline::RunContext;

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
    /// Builds the page sections; `None` when the analyzer produced no report
    /// (it gets no page but stays in `analyzer_ids`).
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
    // The pages show the code that exists: ended entities stay out.
    let report = shotness_run::shotness_run_report_data(ctx.matches)?.live();
    Some(history_shotness::sections(&report.nodes, &report.counters))
}

fn couples_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let data = couples_run::couples_plot_data(ctx.matches)?;
    Some(history_couples::sections(
        &data.file_coupling,
        &data.ownership,
    ))
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
    let metrics = history::anomaly_run_metrics(ctx.matches)?;

    // Cross-analyzer enrichment: anomalies over the OTHER selected
    // analyzers' time series. Only quality and sentiment provide one.
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
            if let Some((ticks, dims)) =
                cf_sentiment::store::extract_store_time_series(&sm.time_series)
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
    let agg = burndown_ndjson::burndown_run_aggregate(ctx.matches)?;
    // Clamp negative dense values to zero.
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
        sampling: agg.grid.sampling,
        granularity: agg.grid.granularity,
        tick_size_ns: agg.tick_size_ns,
        end_time_ns: agg.end_time_ns,
        project_name: agg.project_name.clone(),
    };
    Some(history_burndown::sections(&chart_data, &agg.metrics))
}

fn devs_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = history::devs_run_metrics(ctx.matches)?;
    Some(history_devs::sections(&metrics))
}

fn file_history_sections(ctx: &RunContext, _selected: &[&'static str]) -> Option<Vec<Section>> {
    let metrics = history::file_history_run_metrics(ctx.matches)?;
    Some(history_file_history::sections(
        &metrics.file_churn,
        &metrics.composition_ts,
    ))
}

/// The plot-capable history analyzers in the separate-phase emit order
/// (see the module docs).
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
/// phases. Returns `None` when any selected id has no plot entry (the caller
/// reports that no pages were produced), `Some(0)` on success, `Some(1)` on
/// an I/O failure.
#[must_use]
pub fn run_history_plot(
    ctx: &RunContext,
    history_ids: &[String],
    output_dir: &str,
    mixed: bool,
) -> Option<i32> {
    // Resolve the selection against the registry, keeping the canonical emit
    // order.
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

/// Renders pages + index + report.json (+ the rebuilt cross-phase index of a
/// mixed run).
fn render_history_plot(
    ctx: &RunContext,
    selected: &[&HistoryPlotAnalyzer],
    selected_flags: &[&'static str],
    output_dir: &str,
    mixed: bool,
) -> io::Result<()> {
    create_output_dir(output_dir)?;
    let renderer = page_renderer(output_dir);

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
        // An analyzer without a report gets no page (it stays in
        // analyzer_ids); empty sections still render a page.
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
        // Rescan the output dir and regenerate the unified title-sorted index
        // across both phases.
        renderer.rebuild_index()?;
    }

    Ok(())
}

/// `report.json` of the history phase: `{"analyzer_ids": [...], "pages":
/// [{"ID", "Title", "Description"}...]}` (capitalized page keys), written by
/// [`write_report_json_atomic`].
fn write_render_report_json(
    output_dir: &str,
    analyzer_ids: &[&'static str],
    pages: &[PageMeta],
) -> io::Result<()> {
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push(
        "analyzer_ids",
        GoValue::Array(
            analyzer_ids
                .iter()
                .map(|id| GoValue::Str((*id).to_string()))
                .collect(),
        ),
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
    write_report_json_atomic(output_dir, &GoValue::Map(root))
}
