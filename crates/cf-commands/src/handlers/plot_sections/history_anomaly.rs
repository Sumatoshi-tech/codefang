//! `history/anomaly` plot sections.
//! (`GenerateStoreSections` → `buildStoreSections` over the `time_series`,
//! `anomaly_record`, `aggregate` and — after the plot pipeline's
//! `EnrichAndRewrite` — `external_summary` store kinds).

use std::collections::BTreeMap;

use cf_anomaly::model::{ComputedMetrics, ExternalSummary};
use cf_gojson::GoValue;
use cf_plotpage::components::BadgeColor;
use cf_plotpage::echarts::{Chart, ChartKind, ItemStyle, LineData, LineStyle};
use cf_plotpage::{get_chart_palette, ChartOpts, Hint, Section, Theme};

use crate::handlers::plot_sections::history_shared::{format_float, GridStats};

/// Reference anomaly plot-section constants.
const LINE_WIDTH: f64 = 2.0;
const ANOMALY_RATE_WARNING_THRESHOLD: f64 = 10.0;
const ANOMALY_RATE_ERROR_THRESHOLD: f64 = 25.0;
const MAX_STATS_COLUMNS: usize = 4;

/// The reference `GenerateStoreSections` → `buildStoreSections`: an empty time series
/// yields zero sections. `external_summaries` carries the cross-analyzer
/// enrichment products (empty on a single-analyzer run).
pub fn sections(metrics: &ComputedMetrics, external_summaries: &[ExternalSummary]) -> Vec<Section> {
    if metrics.time_series.is_empty() {
        return Vec::new();
    }

    // buildTickMetricsFromTimeSeries + mapx.SortedKeys: net churn by tick.
    let mut tick_metrics: BTreeMap<i64, i64> = BTreeMap::new();
    for ts in &metrics.time_series {
        tick_metrics.insert(ts.tick, ts.metrics.net_churn);
    }
    if tick_metrics.is_empty() {
        return Vec::new();
    }

    let anomaly_ticks: std::collections::BTreeSet<i64> =
        metrics.anomalies.iter().map(|a| a.tick).collect();

    // buildChartData.
    let mut labels: Vec<String> = Vec::with_capacity(tick_metrics.len());
    let mut churn_data: Vec<GoValue> = Vec::with_capacity(tick_metrics.len());
    let mut anomaly_data: Vec<GoValue> = Vec::with_capacity(tick_metrics.len());
    for (tick, net_churn) in &tick_metrics {
        labels.push(tick.to_string());
        churn_data.push(
            LineData {
                value: Some(GoValue::Int(*net_churn)),
                ..LineData::default()
            }
            .value(),
        );
        if anomaly_ticks.contains(tick) {
            anomaly_data.push(
                LineData {
                    value: Some(GoValue::Int(*net_churn)),
                    symbol: "circle".to_string(),
                    ..LineData::default()
                }
                .value(),
            );
        } else {
            anomaly_data.push(
                LineData {
                    value: Some(GoValue::Str("-".to_string())),
                    ..LineData::default()
                }
                .value(),
            );
        }
    }

    let chart = create_churn_chart(&labels, churn_data, anomaly_data);
    let stat_section = build_stats_section(metrics);

    let mut sections = vec![
        Section {
            title: "Net Churn Over Time with Anomalies".to_string(),
            subtitle: "Lines added minus lines removed per tick; anomalous ticks highlighted."
                .to_string(),
            chart: Some(Box::new(chart)),
            hint: Hint {
                title: "How to interpret:".to_string(),
                items: vec![
                    "Blue line shows net code churn (lines added - lines removed) per time tick"
                        .to_string(),
                    "Red scatter points mark ticks flagged as anomalous (Z-score > threshold)"
                        .to_string(),
                    "Anomalies indicate sudden deviations from the rolling average".to_string(),
                    "Investigate anomaly ticks for large refactors, bulk imports, or regressions"
                        .to_string(),
                    "Adjust --anomaly-threshold to tune sensitivity (lower = more sensitive)"
                        .to_string(),
                    "Adjust --anomaly-window to change the rolling baseline period".to_string(),
                ],
            },
        },
        stat_section,
    ];

    if let Some(ext) = build_external_anomaly_section(external_summaries) {
        sections.push(ext);
    }

    sections
}

/// The reference `createChurnChart`.
fn create_churn_chart(
    labels: &[String],
    churn_data: Vec<GoValue>,
    anomaly_data: Vec<GoValue>,
) -> Chart {
    let co = ChartOpts::default_dark();
    let palette = get_chart_palette(Theme::Dark);

    let mut line = Chart::new(ChartKind::Line);
    let (w, h, bg, theme) = co.init("100%", "500px");
    line.set_init(&w, &h, &bg, &theme);
    line.tooltip = co.tooltip("axis");
    line.data_zoom = co.data_zoom();
    line.x_axis = co.x_axis("Time (tick)");
    line.y_axis = co.y_axis("Net Churn (lines)");
    line.grid = vec![co.grid()];
    line.set_x_axis_labels(labels);

    let s = line.add_series("Net Churn", GoValue::Array(churn_data));
    s.smooth = Some(true);
    s.item_style = Some(ItemStyle {
        color: palette.semantic.good.to_string(),
        ..ItemStyle::default()
    });
    s.line_style = Some(LineStyle {
        width: LINE_WIDTH,
        ..LineStyle::default()
    });

    let s = line.add_series("Anomalies", GoValue::Array(anomaly_data));
    // Reference: WithLineChartOpts(opts.LineChart{Step: ""}) — Step is an interface{}
    // holding the EMPTY string, which is non-nil and therefore EMITTED.
    s.step = Some(GoValue::Str(String::new()));
    s.item_style = Some(ItemStyle {
        color: palette.semantic.bad.to_string(),
        ..ItemStyle::default()
    });
    // Width 0 is the float omitempty zero (skipped); Opacity 0 is a set
    // pointer (emitted).
    s.line_style = Some(LineStyle {
        opacity: Some(0.0),
        ..LineStyle::default()
    });

    line
}

/// The reference `buildStatsSectionFromAggregate`.
fn build_stats_section(metrics: &ComputedMetrics) -> Section {
    let agg = &metrics.aggregate;
    let anomaly_rate_str = format!("{}%", format_float(agg.anomaly_rate, 1));

    let highest_z = metrics
        .anomalies
        .first()
        .map_or_else(|| "N/A".to_string(), |a| format_float(a.max_abs_z_score, 1));

    let mut trend_color = BadgeColor::Success;
    if agg.anomaly_rate > ANOMALY_RATE_WARNING_THRESHOLD {
        trend_color = BadgeColor::Warning;
    }
    if agg.anomaly_rate > ANOMALY_RATE_ERROR_THRESHOLD {
        trend_color = BadgeColor::Error;
    }

    let grid = GridStats::new(MAX_STATS_COLUMNS)
        .stat("Total Ticks", &agg.total_ticks.to_string())
        .stat("Anomalies Detected", &agg.total_anomalies.to_string())
        .stat_with_trend(
            "Anomaly Rate",
            &anomaly_rate_str,
            &anomaly_rate_str,
            trend_color,
        )
        .stat("Highest Z-Score", &highest_z)
        .stat(
            "Avg Language Diversity",
            &format_float(agg.lang_diversity_mean, 1),
        )
        .stat("Avg Author Count", &format_float(agg.author_count_mean, 1))
        .into_grid();

    Section {
        title: "Anomaly Detection Summary".to_string(),
        subtitle: "Aggregate statistics from temporal anomaly analysis.".to_string(),
        chart: Some(Box::new(grid)),
        hint: Hint::default(),
    }
}

/// The reference `buildExternalAnomalySection`: `None` when there are no external
/// summaries (single-analyzer runs).
fn build_external_anomaly_section(summaries: &[ExternalSummary]) -> Option<Section> {
    if summaries.is_empty() {
        return None;
    }

    let mut grid = GridStats::new(MAX_STATS_COLUMNS);
    for summary in summaries {
        let label = format!("{} / {}", summary.source, summary.dimension);
        let value = summary.anomalies.to_string();
        if summary.anomalies > 0 {
            let z_str = format!("peak Z={}", format_float(summary.highest_z, 1));
            grid = grid.stat_with_trend(&label, &value, &z_str, BadgeColor::Warning);
        } else {
            grid = grid.stat(&label, &value);
        }
    }

    Some(Section {
        title: "Cross-Analyzer Anomaly Detection".to_string(),
        subtitle: "Anomalies detected on time series from other history analyzers.".to_string(),
        chart: Some(Box::new(grid.into_grid())),
        hint: Hint::default(),
    })
}
