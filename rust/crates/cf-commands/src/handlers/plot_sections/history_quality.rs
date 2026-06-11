//! `history/quality` plot sections — port of Go
//! `internal/analyzers/quality/store_reader.go` + `plot.go`
//! (`GenerateStoreSections` → `buildStoreSections` over the `time_series` +
//! `aggregate` store kinds, which are exactly the run's `ComputedMetrics`).

use cf_gojson::GoValue;
use cf_plotpage::echarts::{Chart, ChartKind, ItemStyle, LineData, LineStyle};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::plot_sections::history_shared::{format_float, GridStats};

/// quality plot.go constants.
const LINE_WIDTH: f64 = 2.0;
const LINE_WIDTH_THIN: f64 = 1.0;
const EMPTY_CHART_HEIGHT: &str = "400px";
const MAX_STATS_COLUMNS: usize = 4;

/// One line series to plot (Go `chartSeries`).
struct ChartSeries {
    name: &'static str,
    values: Vec<f64>,
    color: String,
    width: f64,
    dashed: bool,
}

/// Go `GenerateStoreSections`: an empty time series yields zero sections.
pub fn sections(metrics: &cf_quality::ComputedMetrics) -> Vec<Section> {
    if metrics.time_series.is_empty() {
        return Vec::new();
    }

    vec![
        Section {
            title: "Cyclomatic Complexity Over Time".to_string(),
            subtitle: "Median, mean, and P95 cyclomatic complexity per tick.".to_string(),
            chart: Some(Box::new(build_complexity_chart(metrics))),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "<b>Median</b> (solid) — robust central tendency, resistant to outliers"
                        .to_string(),
                    "<b>Mean</b> (dashed) — pulled up by complex outlier files; gap with median reveals skew"
                        .to_string(),
                    "<b>P95</b> (dotted) — the 95th percentile; shows worst-case complexity trend"
                        .to_string(),
                    "Rising median trend indicates overall code is becoming harder to maintain"
                        .to_string(),
                    "Large mean/median gap reveals heavy-tailed outliers (generated code, bulk imports)"
                        .to_string(),
                ],
            },
        },
        Section {
            title: "Halstead Volume Over Time".to_string(),
            subtitle: "Median, mean, and P95 Halstead volume per tick.".to_string(),
            chart: Some(Box::new(build_halstead_chart(metrics))),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Halstead volume measures code size and complexity in information-theoretic terms"
                        .to_string(),
                    "<b>Median</b> shows typical file complexity; <b>P95</b> shows outlier magnitude"
                        .to_string(),
                    "Large gap between mean and median indicates a few very large/complex files dominate"
                        .to_string(),
                ],
            },
        },
        build_quality_stats_section(metrics),
    ]
}

/// Go `buildComplexityChart`.
fn build_complexity_chart(metrics: &cf_quality::ComputedMetrics) -> Chart {
    let palette = get_chart_palette(Theme::Dark);
    build_distribution_chart(
        metrics,
        "Complexity Over Time",
        "Complexity",
        |s| s.complexity_median,
        |s| s.complexity_mean,
        |s| s.complexity_p95,
        palette.semantic.good,
    )
}

/// Go `buildHalsteadChart`.
fn build_halstead_chart(metrics: &cf_quality::ComputedMetrics) -> Chart {
    let palette = get_chart_palette(Theme::Dark);
    build_distribution_chart(
        metrics,
        "Halstead Volume Over Time",
        "Halstead Volume",
        |s| s.halstead_vol_median,
        |s| s.halstead_vol_mean,
        |s| s.halstead_vol_p95,
        palette.semantic.warning,
    )
}

/// Go `buildDistributionChart`.
fn build_distribution_chart(
    metrics: &cf_quality::ComputedMetrics,
    title: &str,
    y_axis_label: &str,
    median: fn(&cf_quality::TickStats) -> f64,
    mean: fn(&cf_quality::TickStats) -> f64,
    p95: fn(&cf_quality::TickStats) -> f64,
    median_color: &str,
) -> Chart {
    let palette = get_chart_palette(Theme::Dark);
    let pick = |f: fn(&cf_quality::TickStats) -> f64| -> Vec<f64> {
        metrics.time_series.iter().map(|e| f(&e.stats)).collect()
    };
    build_multi_series_chart(
        metrics,
        title,
        y_axis_label,
        vec![
            ChartSeries {
                name: "Median",
                values: pick(median),
                color: median_color.to_string(),
                width: LINE_WIDTH,
                dashed: false,
            },
            ChartSeries {
                name: "Mean",
                values: pick(mean),
                color: palette.primary[0].to_string(),
                width: LINE_WIDTH_THIN,
                dashed: true,
            },
            ChartSeries {
                name: "P95",
                values: pick(p95),
                color: palette.semantic.bad.to_string(),
                width: LINE_WIDTH_THIN,
                dashed: true,
            },
        ],
    )
}

/// Go `buildMultiSeriesChart`.
fn build_multi_series_chart(
    metrics: &cf_quality::ComputedMetrics,
    title: &str,
    y_axis_label: &str,
    series: Vec<ChartSeries>,
) -> Chart {
    if metrics.time_series.is_empty() {
        return create_empty_chart(title);
    }

    let labels: Vec<String> =
        metrics.time_series.iter().map(|e| e.tick.to_string()).collect();

    let co = ChartOpts::default_dark();
    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", "500px");
    line.set_init(&w, &h, &bg, &theme);
    line.tooltip = co.tooltip("axis");
    line.data_zoom = co.data_zoom();
    line.x_axis = co.x_axis("Time (tick)");
    line.y_axis = co.y_axis(y_axis_label);
    line.grid = vec![co.grid()];
    line.set_x_axis_labels(&labels);

    for s in series {
        let data = GoValue::Array(
            s.values
                .iter()
                .map(|v| {
                    LineData {
                        value: Some(GoValue::Float(*v)),
                        ..LineData::default()
                    }
                    .value()
                })
                .collect(),
        );
        let added = line.add_series(s.name, data);
        added.smooth = Some(true);
        added.item_style = Some(ItemStyle {
            color: s.color.clone(),
            ..ItemStyle::default()
        });
        added.line_style = Some(LineStyle {
            width: s.width,
            type_: if s.dashed { "dashed".to_string() } else { String::new() },
            ..LineStyle::default()
        });
    }

    line
}

/// Go `buildQualityStatsSection`.
fn build_quality_stats_section(metrics: &cf_quality::ComputedMetrics) -> Section {
    let agg = &metrics.aggregate;
    let grid = GridStats::new(MAX_STATS_COLUMNS)
        .stat("Median Complexity", &format_float(agg.complexity_median_mean, 2))
        .stat("P95 Complexity", &format_float(agg.complexity_p95_mean, 2))
        .stat("Median Halstead Vol", &format_float(agg.halstead_vol_median_mean, 1))
        .stat("Total Delivered Bugs", &format_float(agg.total_delivered_bugs, 1))
        .stat("Min Comment Score", &format_float(agg.min_comment_score, 2))
        .stat("Min Cohesion", &format_float(agg.min_cohesion, 2))
        .stat("Total Files Analyzed", &agg.total_files_analyzed.to_string())
        .into_grid();

    Section {
        title: "Code Quality Summary".to_string(),
        subtitle: "Aggregate statistics from code quality analysis across commit history."
            .to_string(),
        chart: Some(Box::new(grid)),
        hint: Hint::default(),
    }
}

/// Go `createEmptyChart`.
fn create_empty_chart(title: &str) -> Chart {
    let co = ChartOpts::default_dark();
    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", EMPTY_CHART_HEIGHT);
    line.set_init(&w, &h, &bg, &theme);
    line.title = co.title(title, "No data");
    line
}
